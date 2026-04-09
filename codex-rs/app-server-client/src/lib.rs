//! Shared in-process app-server client facade for CLI surfaces.
//!
//! This crate wraps [`codex_app_server::in_process`] behind a single async API
//! used by surfaces like TUI and exec. It centralizes:
//!
//! - Runtime startup and initialize-capabilities handshake.
//! - Typed caller-provided startup identity (`SessionSource` + client name).
//! - Typed and raw request/notification dispatch.
//! - Server request resolution and rejection.
//! - Event consumption with backpressure signaling ([`InProcessServerEvent::Lagged`]).
//! - Bounded graceful shutdown with abort fallback.
//!
//! The facade interposes a worker task between the caller and the underlying
//! [`InProcessClientHandle`](codex_app_server::in_process::InProcessClientHandle),
//! bridging async `mpsc` channels on both sides. Queues are bounded so overload
//! surfaces as channel-full errors rather than unbounded memory growth.

mod remote;

use std::error::Error;
use std::fmt;
use std::io::Error as IoError;
use std::io::ErrorKind;
use std::io::Result as IoResult;
use std::sync::Arc;
use std::time::Duration;

pub use codex_app_server::in_process::DEFAULT_IN_PROCESS_CHANNEL_CAPACITY;
pub use codex_app_server::in_process::InProcessServerEvent;
use codex_app_server::in_process::InProcessStartArgs;
use codex_app_server_protocol::ClientInfo;
use codex_app_server_protocol::ClientNotification;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::ConfigWarningNotification;
use codex_app_server_protocol::InitializeCapabilities;
use codex_app_server_protocol::InitializeParams;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::Result as JsonRpcResult;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ServerRequest;
use codex_arg0::Arg0DispatchPaths;
use codex_core::config::Config;
use codex_core::config_loader::CloudRequirementsLoader;
use codex_core::config_loader::LoaderOverrides;
use codex_feedback::CodexFeedback;
use codex_protocol::protocol::SessionSource;
use serde::de::DeserializeOwned;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::time::timeout;
use toml::Value as TomlValue;
use tracing::warn;

pub use crate::remote::RemoteAppServerClient;
pub use crate::remote::RemoteAppServerConnectArgs;

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// Raw app-server request result for typed in-process requests.
///
/// Even on the in-process path, successful responses still travel back through
/// the same JSON-RPC result envelope used by socket/stdio transports because
/// `MessageProcessor` continues to produce that shape internally.
pub type RequestResult = std::result::Result<JsonRpcResult, JSONRPCErrorError>;

#[derive(Debug, Clone)]
pub enum AppServerEvent {
    Lagged { skipped: usize },
    ServerNotification(ServerNotification),
    ServerRequest(ServerRequest),
    Disconnected { message: String },
}

impl From<InProcessServerEvent> for AppServerEvent {
    fn from(value: InProcessServerEvent) -> Self {
        match value {
            InProcessServerEvent::Lagged { skipped } => Self::Lagged { skipped },
            InProcessServerEvent::ServerNotification(notification) => {
                Self::ServerNotification(notification)
            }
            InProcessServerEvent::ServerRequest(request) => Self::ServerRequest(request),
        }
    }
}

fn event_requires_delivery(event: &InProcessServerEvent) -> bool {
    // These transcript and terminal events must remain lossless. Dropping
    // streamed assistant text or the authoritative completed item can leave
    // the TUI with permanently corrupted markdown, while dropping completion
    // notifications can leave surfaces waiting forever.
    match event {
        InProcessServerEvent::ServerNotification(notification) => {
            server_notification_requires_delivery(notification)
        }
        _ => false,
    }
}

/// Returns `true` for notifications that must survive backpressure.
///
/// Transcript events (`AgentMessageDelta`, `PlanDelta`, reasoning deltas) and
/// the authoritative `ItemCompleted` / `TurnCompleted` form the lossless tier
/// of the event stream. Dropping any of these corrupts the visible assistant
/// output or leaves surfaces waiting for a completion signal that already
/// fired. Everything else (`CommandExecutionOutputDelta`, progress, etc.) is
/// best-effort and may be dropped with only cosmetic impact.
///
/// Both the in-process and remote transports delegate to this function so the
/// classification stays in sync.
pub(crate) fn server_notification_requires_delivery(notification: &ServerNotification) -> bool {
    matches!(
        notification,
        ServerNotification::TurnCompleted(_)
            | ServerNotification::ItemCompleted(_)
            | ServerNotification::AgentMessageDelta(_)
            | ServerNotification::PlanDelta(_)
            | ServerNotification::ReasoningSummaryTextDelta(_)
            | ServerNotification::ReasoningTextDelta(_)
    )
}

/// Outcome of attempting to forward a single event to the consumer channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ForwardEventResult {
    /// The event was delivered (or intentionally dropped); the stream is healthy.
    Continue,
    /// The consumer channel is closed; the caller should stop producing events.
    DisableStream,
}

/// Forwards a single in-process event to the consumer, respecting the
/// lossless/best-effort split.
///
/// Lossless events (transcript deltas, item/turn completions) block until the
/// consumer drains capacity. Best-effort events use `try_send` and increment
/// `skipped_events` on failure. When a lag marker needs to be flushed before a
/// lossless event, the flush itself blocks so the marker is never lost.
///
/// If a dropped event is a `ServerRequest`, `reject_server_request` is called
/// so the server does not wait for a response that will never come.
async fn forward_in_process_event<F>(
    event_tx: &mpsc::Sender<InProcessServerEvent>,
    skipped_events: &mut usize,
    event: InProcessServerEvent,
    mut reject_server_request: F,
) -> ForwardEventResult
where
    F: FnMut(ServerRequest),
{
    if *skipped_events > 0 {
        if event_requires_delivery(&event) {
            // Surface lag before the lossless event, but do not let the lag marker itself cause
            // us to drop the transcript/completion notification the caller is blocked on.
            if event_tx
                .send(InProcessServerEvent::Lagged {
                    skipped: *skipped_events,
                })
                .await
                .is_err()
            {
                return ForwardEventResult::DisableStream;
            }
            *skipped_events = 0;
        } else {
            match event_tx.try_send(InProcessServerEvent::Lagged {
                skipped: *skipped_events,
            }) {
                Ok(()) => {
                    *skipped_events = 0;
                }
                Err(mpsc::error::TrySendError::Full(_)) => {
                    *skipped_events = skipped_events.saturating_add(1);
                    warn!("dropping in-process app-server event because consumer queue is full");
                    if let InProcessServerEvent::ServerRequest(request) = event {
                        reject_server_request(request);
                    }
                    return ForwardEventResult::Continue;
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    return ForwardEventResult::DisableStream;
                }
            }
        }
    }

    if event_requires_delivery(&event) {
        // Block until the consumer catches up for transcript/completion notifications; this
        // preserves the visible assistant output even when the queue is otherwise saturated.
        if event_tx.send(event).await.is_err() {
            return ForwardEventResult::DisableStream;
        }
        return ForwardEventResult::Continue;
    }

    match event_tx.try_send(event) {
        Ok(()) => ForwardEventResult::Continue,
        Err(mpsc::error::TrySendError::Full(event)) => {
            *skipped_events = skipped_events.saturating_add(1);
            warn!("dropping in-process app-server event because consumer queue is full");
            if let InProcessServerEvent::ServerRequest(request) = event {
                reject_server_request(request);
            }
            ForwardEventResult::Continue
        }
        Err(mpsc::error::TrySendError::Closed(_)) => ForwardEventResult::DisableStream,
    }
}

/// Layered error for [`InProcessAppServerClient::request_typed`].
///
/// This keeps transport failures, server-side JSON-RPC failures, and response
/// decode failures distinct so callers can decide whether to retry, surface a
/// server error, or treat the response as an internal request/response mismatch.
#[derive(Debug)]
pub enum TypedRequestError {
    Transport {
        method: String,
        source: IoError,
    },
    Server {
        method: String,
        source: JSONRPCErrorError,
    },
    Deserialize {
        method: String,
        source: serde_json::Error,
    },
}

impl fmt::Display for TypedRequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport { method, source } => {
                write!(f, "{method} transport error: {source}")
            }
            Self::Server { method, source } => {
                write!(f, "{method} failed: {}", source.message)
            }
            Self::Deserialize { method, source } => {
                write!(f, "{method} response decode error: {source}")
            }
        }
    }
}

impl Error for TypedRequestError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Transport { source, .. } => Some(source),
            Self::Server { .. } => None,
            Self::Deserialize { source, .. } => Some(source),
        }
    }
}

#[derive(Clone)]
pub struct InProcessClientStartArgs {
    /// Resolved argv0 dispatch paths used by command execution internals.
    pub arg0_paths: Arg0DispatchPaths,
    /// Shared config used to initialize app-server runtime.
    pub config: Arc<Config>,
    /// CLI config overrides that are already parsed into TOML values.
    pub cli_overrides: Vec<(String, TomlValue)>,
    /// Loader override knobs used by config API paths.
    pub loader_overrides: LoaderOverrides,
    /// Preloaded cloud requirements provider.
    pub cloud_requirements: CloudRequirementsLoader,
    /// Feedback sink used by app-server/core telemetry and logs.
    pub feedback: CodexFeedback,
    /// Startup warnings emitted after initialize succeeds.
    pub config_warnings: Vec<ConfigWarningNotification>,
    /// Session source recorded in app-server thread metadata.
    pub session_source: SessionSource,
    /// Whether auth loading should honor the `CODEX_API_KEY` environment variable.
    pub enable_codex_api_key_env: bool,
    /// Client name reported during initialize.
    pub client_name: String,
    /// Client version reported during initialize.
    pub client_version: String,
    /// Whether experimental APIs are requested at initialize time.
    pub experimental_api: bool,
    /// Notification methods this client opts out of receiving.
    pub opt_out_notification_methods: Vec<String>,
    /// Queue capacity for command/event channels (clamped to at least 1).
    pub channel_capacity: usize,
}

impl InProcessClientStartArgs {
    /// Builds initialize params from caller-provided metadata.
    pub fn initialize_params(&self) -> InitializeParams {
        let capabilities = InitializeCapabilities {
            experimental_api: self.experimental_api,
            opt_out_notification_methods: if self.opt_out_notification_methods.is_empty() {
                None
            } else {
                Some(self.opt_out_notification_methods.clone())
            },
        };

        InitializeParams {
            client_info: ClientInfo {
                name: self.client_name.clone(),
                title: None,
                version: self.client_version.clone(),
            },
            capabilities: Some(capabilities),
        }
    }

    fn into_runtime_start_args(self) -> InProcessStartArgs {
        let initialize = self.initialize_params();
        InProcessStartArgs {
            arg0_paths: self.arg0_paths,
            config: self.config,
            cli_overrides: self.cli_overrides,
            loader_overrides: self.loader_overrides,
            cloud_requirements: self.cloud_requirements,
            feedback: self.feedback,
            config_warnings: self.config_warnings,
            session_source: self.session_source,
            enable_codex_api_key_env: self.enable_codex_api_key_env,
            initialize,
            channel_capacity: self.channel_capacity,
        }
    }
}

/// Internal command sent from public facade methods to the worker task.
///
/// Each variant carries a oneshot sender so the caller can `await` the
/// result without holding a mutable reference to the client.
enum ClientCommand {
    Request {
        request: Box<ClientRequest>,
        response_tx: oneshot::Sender<IoResult<RequestResult>>,
    },
    Notify {
        notification: ClientNotification,
        response_tx: oneshot::Sender<IoResult<()>>,
    },
    ResolveServerRequest {
        request_id: RequestId,
        result: JsonRpcResult,
        response_tx: oneshot::Sender<IoResult<()>>,
    },
    RejectServerRequest {
        request_id: RequestId,
        error: JSONRPCErrorError,
        response_tx: oneshot::Sender<IoResult<()>>,
    },
    Shutdown {
        response_tx: oneshot::Sender<IoResult<()>>,
    },
}

/// Async facade over the in-process app-server runtime.
///
/// This type owns a worker task that bridges between:
/// - caller-facing async `mpsc` channels used by TUI/exec
/// - [`codex_app_server::in_process::InProcessClientHandle`], which speaks to
///   the embedded `MessageProcessor`
///
/// The facade intentionally preserves the server's request/notification/event
/// model instead of exposing direct core runtime handles. That keeps in-process
/// callers aligned with app-server behavior while still avoiding a process
/// boundary.
pub struct InProcessAppServerClient {
    command_tx: mpsc::Sender<ClientCommand>,
    event_rx: mpsc::Receiver<InProcessServerEvent>,
    worker_handle: tokio::task::JoinHandle<()>,
}

#[derive(Clone)]
pub struct InProcessAppServerRequestHandle {
    command_tx: mpsc::Sender<ClientCommand>,
}

#[derive(Clone)]
pub enum AppServerRequestHandle {
    InProcess(InProcessAppServerRequestHandle),
    Remote(crate::remote::RemoteAppServerRequestHandle),
}

pub enum AppServerClient {
    InProcess(InProcessAppServerClient),
    Remote(RemoteAppServerClient),
}

impl InProcessAppServerClient {
    /// Starts the in-process runtime and facade worker task.
    ///
    /// The returned client is ready for requests and event consumption. If the
    /// internal event queue is saturated later, server requests are rejected
    /// with overload error instead of being silently dropped.
    pub async fn start(args: InProcessClientStartArgs) -> IoResult<Self> {
        let channel_capacity = args.channel_capacity.max(1);
        let mut handle =
            codex_app_server::in_process::start(args.into_runtime_start_args()).await?;
        let request_sender = handle.sender();
        let (command_tx, mut command_rx) = mpsc::channel::<ClientCommand>(channel_capacity);
        let (event_tx, event_rx) = mpsc::channel::<InProcessServerEvent>(channel_capacity);

        let worker_handle = tokio::spawn(async move {
            let mut event_stream_enabled = true;
            let mut skipped_events = 0usize;
            loop {
                tokio::select! {
                    command = command_rx.recv() => {
                        match command {
                            Some(ClientCommand::Request { request, response_tx }) => {
                                let request_sender = request_sender.clone();
                                // Request waits happen on a detached task so
                                // this loop can keep draining runtime events
                                // while the request is blocked on client input.
                                tokio::spawn(async move {
                                    let result = request_sender.request(*request).await;
                                    let _ = response_tx.send(result);
                                });
                            }
                            Some(ClientCommand::Notify {
                                notification,
                                response_tx,
                            }) => {
                                let result = request_sender.notify(notification);
                                let _ = response_tx.send(result);
                            }
                            Some(ClientCommand::ResolveServerRequest {
                                request_id,
                                result,
                                response_tx,
                            }) => {
                                let send_result =
                                    request_sender.respond_to_server_request(request_id, result);
                                let _ = response_tx.send(send_result);
                            }
                            Some(ClientCommand::RejectServerRequest {
                                request_id,
                                error,
                                response_tx,
                            }) => {
                                let send_result = request_sender.fail_server_request(request_id, error);
                                let _ = response_tx.send(send_result);
                            }
                            Some(ClientCommand::Shutdown { response_tx }) => {
                                let shutdown_result = handle.shutdown().await;
                                let _ = response_tx.send(shutdown_result);
                                break;
                            }
                            None => {
                                let _ = handle.shutdown().await;
                                break;
                            }
                        }
                    }
                    event = handle.next_event(), if event_stream_enabled => {
                        let Some(event) = event else {
                            break;
                        };
                        if let InProcessServerEvent::ServerRequest(
                            ServerRequest::ChatgptAuthTokensRefresh { request_id, .. }
                        ) = &event
                        {
                            let send_result = request_sender.fail_server_request(
                                request_id.clone(),
                                JSONRPCErrorError {
                                    code: -32000,
                                    message: "chatgpt auth token refresh is not supported for in-process app-server clients".to_string(),
                                    data: None,
                                },
                            );
                            if let Err(err) = send_result {
                                warn!(
                                    "failed to reject unsupported chatgpt auth token refresh request: {err}"
                                );
                            }
                            continue;
                        }

                        match forward_in_process_event(
                            &event_tx,
                            &mut skipped_events,
                            event,
                            |request| {
                                let _ = request_sender.fail_server_request(
                                    request.id().clone(),
                                    JSONRPCErrorError {
                                        code: -32001,
                                        message: "in-process app-server event queue is full"
                                            .to_string(),
                                        data: None,
                                    },
                                );
                            },
                        )
                        .await
                        {
                            ForwardEventResult::Continue => {}
                            ForwardEventResult::DisableStream => {
                                event_stream_enabled = false;
                            }
                        }
                    }
                }
            }
        });

        Ok(Self {
            command_tx,
            event_rx,
            worker_handle,
        })
    }

    pub fn request_handle(&self) -> InProcessAppServerRequestHandle {
        InProcessAppServerRequestHandle {
            command_tx: self.command_tx.clone(),
        }
    }

    /// Sends a typed client request and returns raw JSON-RPC result.
    ///
    /// Callers that expect a concrete response type should usually prefer
    /// [`request_typed`](Self::request_typed).
    pub async fn request(&self, request: ClientRequest) -> IoResult<RequestResult> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(ClientCommand::Request {
                request: Box::new(request),
                response_tx,
            })
            .await
            .map_err(|_| {
                IoError::new(
                    ErrorKind::BrokenPipe,
                    "in-process app-server worker channel is closed",
                )
            })?;
        response_rx.await.map_err(|_| {
            IoError::new(
                ErrorKind::BrokenPipe,
                "in-process app-server request channel is closed",
            )
        })?
    }

    /// Sends a typed client request and decodes the successful response body.
    ///
    /// This still deserializes from a JSON value produced by app-server's
    /// JSON-RPC result envelope. Because the caller chooses `T`, `Deserialize`
    /// failures indicate an internal request/response mismatch at the call site
    /// (or an in-process bug), not transport skew from an external client.
    pub async fn request_typed<T>(&self, request: ClientRequest) -> Result<T, TypedRequestError>
    where
        T: DeserializeOwned,
    {
        let method = request_method_name(&request);
        let response =
            self.request(request)
                .await
                .map_err(|source| TypedRequestError::Transport {
                    method: method.clone(),
                    source,
                })?;
        let result = response.map_err(|source| TypedRequestError::Server {
            method: method.clone(),
            source,
        })?;
        serde_json::from_value(result)
            .map_err(|source| TypedRequestError::Deserialize { method, source })
    }

    /// Sends a typed client notification.
    pub async fn notify(&self, notification: ClientNotification) -> IoResult<()> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(ClientCommand::Notify {
                notification,
                response_tx,
            })
            .await
            .map_err(|_| {
                IoError::new(
                    ErrorKind::BrokenPipe,
                    "in-process app-server worker channel is closed",
                )
            })?;
        response_rx.await.map_err(|_| {
            IoError::new(
                ErrorKind::BrokenPipe,
                "in-process app-server notify channel is closed",
            )
        })?
    }

    /// Resolves a pending server request.
    ///
    /// This should only be called with request IDs obtained from the current
    /// client's event stream.
    pub async fn resolve_server_request(
        &self,
        request_id: RequestId,
        result: JsonRpcResult,
    ) -> IoResult<()> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(ClientCommand::ResolveServerRequest {
                request_id,
                result,
                response_tx,
            })
            .await
            .map_err(|_| {
                IoError::new(
                    ErrorKind::BrokenPipe,
                    "in-process app-server worker channel is closed",
                )
            })?;
        response_rx.await.map_err(|_| {
            IoError::new(
                ErrorKind::BrokenPipe,
                "in-process app-server resolve channel is closed",
            )
        })?
    }

    /// Rejects a pending server request with JSON-RPC error payload.
    pub async fn reject_server_request(
        &self,
        request_id: RequestId,
        error: JSONRPCErrorError,
    ) -> IoResult<()> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(ClientCommand::RejectServerRequest {
                request_id,
                error,
                response_tx,
            })
            .await
            .map_err(|_| {
                IoError::new(
                    ErrorKind::BrokenPipe,
                    "in-process app-server worker channel is closed",
                )
            })?;
        response_rx.await.map_err(|_| {
            IoError::new(
                ErrorKind::BrokenPipe,
                "in-process app-server reject channel is closed",
            )
        })?
    }

    /// Returns the next in-process event, or `None` when worker exits.
    ///
    /// Callers are expected to drain this stream promptly. If they fall behind,
    /// the worker emits [`InProcessServerEvent::Lagged`] markers and may reject
    /// pending server requests rather than letting approval flows hang.
    pub async fn next_event(&mut self) -> Option<InProcessServerEvent> {
        self.event_rx.recv().await
    }

    /// Shuts down worker and in-process runtime with bounded wait.
    ///
    /// If graceful shutdown exceeds timeout, the worker task is aborted to
    /// avoid leaking background tasks in embedding callers.
    pub async fn shutdown(self) -> IoResult<()> {
        let Self {
            command_tx,
            event_rx,
            worker_handle,
        } = self;
        let mut worker_handle = worker_handle;
        // Drop the caller-facing receiver before asking the worker to shut
        // down. That unblocks any pending must-deliver `event_tx.send(..)`
        // so the worker can reach `handle.shutdown()` instead of timing out
        // and getting aborted with the runtime still attached.
        drop(event_rx);
        let (response_tx, response_rx) = oneshot::channel();
        if command_tx
            .send(ClientCommand::Shutdown { response_tx })
            .await
            .is_ok()
            && let Ok(command_result) = timeout(SHUTDOWN_TIMEOUT, response_rx).await
        {
            command_result.map_err(|_| {
                IoError::new(
                    ErrorKind::BrokenPipe,
                    "in-process app-server shutdown channel is closed",
                )
            })??;
        }

        if let Err(_elapsed) = timeout(SHUTDOWN_TIMEOUT, &mut worker_handle).await {
            worker_handle.abort();
            let _ = worker_handle.await;
        }
        Ok(())
    }
}

impl InProcessAppServerRequestHandle {
    pub async fn request(&self, request: ClientRequest) -> IoResult<RequestResult> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(ClientCommand::Request {
                request: Box::new(request),
                response_tx,
            })
            .await
            .map_err(|_| {
                IoError::new(
                    ErrorKind::BrokenPipe,
                    "in-process app-server worker channel is closed",
                )
            })?;
        response_rx.await.map_err(|_| {
            IoError::new(
                ErrorKind::BrokenPipe,
                "in-process app-server request channel is closed",
            )
        })?
    }

    pub async fn request_typed<T>(&self, request: ClientRequest) -> Result<T, TypedRequestError>
    where
        T: DeserializeOwned,
    {
        let method = request_method_name(&request);
        let response =
            self.request(request)
                .await
                .map_err(|source| TypedRequestError::Transport {
                    method: method.clone(),
                    source,
                })?;
        let result = response.map_err(|source| TypedRequestError::Server {
            method: method.clone(),
            source,
        })?;
        serde_json::from_value(result)
            .map_err(|source| TypedRequestError::Deserialize { method, source })
    }
}

impl AppServerRequestHandle {
    pub async fn request(&self, request: ClientRequest) -> IoResult<RequestResult> {
        match self {
            Self::InProcess(handle) => handle.request(request).await,
            Self::Remote(handle) => handle.request(request).await,
        }
    }

    pub async fn request_typed<T>(&self, request: ClientRequest) -> Result<T, TypedRequestError>
    where
        T: DeserializeOwned,
    {
        match self {
            Self::InProcess(handle) => handle.request_typed(request).await,
            Self::Remote(handle) => handle.request_typed(request).await,
        }
    }
}

impl AppServerClient {
    pub async fn request(&self, request: ClientRequest) -> IoResult<RequestResult> {
        match self {
            Self::InProcess(client) => client.request(request).await,
            Self::Remote(client) => client.request(request).await,
        }
    }

    pub async fn request_typed<T>(&self, request: ClientRequest) -> Result<T, TypedRequestError>
    where
        T: DeserializeOwned,
    {
        match self {
            Self::InProcess(client) => client.request_typed(request).await,
            Self::Remote(client) => client.request_typed(request).await,
        }
    }

    pub async fn notify(&self, notification: ClientNotification) -> IoResult<()> {
        match self {
            Self::InProcess(client) => client.notify(notification).await,
            Self::Remote(client) => client.notify(notification).await,
        }
    }

    pub async fn resolve_server_request(
        &self,
        request_id: RequestId,
        result: JsonRpcResult,
    ) -> IoResult<()> {
        match self {
            Self::InProcess(client) => client.resolve_server_request(request_id, result).await,
            Self::Remote(client) => client.resolve_server_request(request_id, result).await,
        }
    }

    pub async fn reject_server_request(
        &self,
        request_id: RequestId,
        error: JSONRPCErrorError,
    ) -> IoResult<()> {
        match self {
            Self::InProcess(client) => client.reject_server_request(request_id, error).await,
            Self::Remote(client) => client.reject_server_request(request_id, error).await,
        }
    }

    pub async fn next_event(&mut self) -> Option<AppServerEvent> {
        match self {
            Self::InProcess(client) => client.next_event().await.map(Into::into),
            Self::Remote(client) => client.next_event().await,
        }
    }

    pub async fn shutdown(self) -> IoResult<()> {
        match self {
            Self::InProcess(client) => client.shutdown().await,
            Self::Remote(client) => client.shutdown().await,
        }
    }

    pub fn request_handle(&self) -> AppServerRequestHandle {
        match self {
            Self::InProcess(client) => AppServerRequestHandle::InProcess(client.request_handle()),
            Self::Remote(client) => AppServerRequestHandle::Remote(client.request_handle()),
        }
    }
}

/// Extracts the JSON-RPC method name for diagnostics without extending the
/// protocol crate with in-process-only helpers.
pub(crate) fn request_method_name(request: &ClientRequest) -> String {
    serde_json::to_value(request)
        .ok()
        .and_then(|value| {
            value
                .get("method")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "<unknown>".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use codex_app_server_protocol::AccountUpdatedNotification;
    use codex_app_server_protocol::ConfigRequirementsReadResponse;
    use codex_app_server_protocol::GetAccountResponse;
    use codex_app_server_protocol::GitInfo;
    use codex_app_server_protocol::JSONRPCMessage;
    use codex_app_server_protocol::JSONRPCRequest;
    use codex_app_server_protocol::JSONRPCResponse;
    use codex_app_server_protocol::ServerNotification;
    use codex_app_server_protocol::SessionSource as ApiSessionSource;
    use codex_app_server_protocol::ThreadActiveFlag;
    use codex_app_server_protocol::ThreadListParams;
    use codex_app_server_protocol::ThreadListResponse;
    use codex_app_server_protocol::ThreadLoadedListParams;
    use codex_app_server_protocol::ThreadLoadedListResponse;
    use codex_app_server_protocol::ThreadLoadedReadParams;
    use codex_app_server_protocol::ThreadLoadedReadResponse;
    use codex_app_server_protocol::ThreadMetadataGitInfoUpdateParams;
    use codex_app_server_protocol::ThreadMetadataUpdateParams;
    use codex_app_server_protocol::ThreadMetadataUpdateResponse;
    use codex_app_server_protocol::ThreadMode;
    use codex_app_server_protocol::ThreadReadParams;
    use codex_app_server_protocol::ThreadReadResponse;
    use codex_app_server_protocol::ThreadResumeResponse;
    use codex_app_server_protocol::ThreadRollbackResponse;
    use codex_app_server_protocol::ThreadStartParams;
    use codex_app_server_protocol::ThreadStartResponse;
    use codex_app_server_protocol::ThreadStatus;
    use codex_app_server_protocol::ThreadUnsubscribeStatus;
    use codex_app_server_protocol::ToolRequestUserInputParams;
    use codex_app_server_protocol::ToolRequestUserInputQuestion;
    use codex_app_server_protocol::TurnStartParams;
    use codex_app_server_protocol::TurnStatus;
    use codex_app_server_protocol::UserInput;
    use codex_core::config::ConfigBuilder;
    use codex_core::state_db_bridge::open_if_present;
    use codex_protocol::ThreadId;
    use codex_protocol::protocol::SessionMeta;
    use codex_protocol::protocol::SessionMetaLine;
    use codex_protocol::protocol::SessionSource as ProtocolSessionSource;
    use codex_state::ThreadMetadataBuilder;
    use core_test_support::responses;
    use futures::SinkExt;
    use futures::StreamExt;
    use pretty_assertions::assert_eq;
    use std::path::Path;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;
    use tokio::net::TcpListener;
    use tokio::time::Duration;
    use tokio::time::timeout;
    use tokio_tungstenite::accept_hdr_async;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::handshake::server::Request as WebSocketRequest;
    use tokio_tungstenite::tungstenite::handshake::server::Response as WebSocketResponse;
    use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;

    async fn build_test_config() -> Config {
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let codex_home_path = std::env::temp_dir().join(format!(
            "codex-app-server-client-test-{}-{unique_suffix}",
            std::process::id()
        ));
        std::fs::create_dir_all(&codex_home_path).expect("temp codex home should exist");

        ConfigBuilder::default()
            .codex_home(codex_home_path)
            .build()
            .await
            .expect("test config should load")
    }

    async fn build_mock_responses_test_config(server_uri: &str) -> Config {
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let codex_home_path = std::env::temp_dir().join(format!(
            "codex-app-server-client-mock-test-{}-{unique_suffix}",
            std::process::id()
        ));
        std::fs::create_dir_all(&codex_home_path).expect("temp codex home should exist");
        std::fs::write(
            codex_home_path.join("config.toml"),
            format!(
                r#"
model = "mock-model"
approval_policy = "never"
sandbox_mode = "read-only"

model_provider = "mock_provider"

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "{server_uri}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
supports_websockets = false
"#
            ),
        )
        .expect("mock config.toml should be writable");

        ConfigBuilder::default()
            .codex_home(codex_home_path)
            .build()
            .await
            .expect("mock test config should load")
    }

    async fn start_test_client_with_capacity(
        session_source: SessionSource,
        channel_capacity: usize,
    ) -> InProcessAppServerClient {
        InProcessAppServerClient::start(InProcessClientStartArgs {
            arg0_paths: Arg0DispatchPaths::default(),
            config: Arc::new(build_test_config().await),
            cli_overrides: Vec::new(),
            loader_overrides: LoaderOverrides::default(),
            cloud_requirements: CloudRequirementsLoader::default(),
            feedback: CodexFeedback::new(),
            config_warnings: Vec::new(),
            session_source,
            enable_codex_api_key_env: false,
            client_name: "codex-app-server-client-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity,
        })
        .await
        .expect("in-process app-server client should start")
    }

    async fn start_test_client(session_source: SessionSource) -> InProcessAppServerClient {
        start_test_client_with_capacity(session_source, DEFAULT_IN_PROCESS_CHANNEL_CAPACITY).await
    }

    async fn mark_state_backfill_complete(config: &Config) {
        let state_runtime = codex_state::StateRuntime::init(
            config.codex_home.clone(),
            config.model_provider_id.clone(),
        )
        .await
        .expect("state runtime should init");
        state_runtime
            .mark_backfill_complete(/*last_watermark*/ None)
            .await
            .expect("backfill marker should persist");
    }

    async fn persist_thread_mode_in_state_db(
        config: &Config,
        started: &ThreadStartResponse,
        mode: &str,
    ) {
        let thread_id = ThreadId::from_string(&started.thread.id).expect("valid thread id");
        let state_db = open_if_present(&config.codex_home, "openai")
            .await
            .expect("state db should exist");
        let rollout_path = started
            .thread
            .path
            .clone()
            .expect("thread/start should expose rollout path");
        let mut metadata = ThreadMetadataBuilder::new(
            thread_id,
            rollout_path,
            Utc::now(),
            ProtocolSessionSource::Cli,
        );
        metadata.mode = mode.to_string();
        metadata.model_provider = Some(started.thread.model_provider.clone());
        metadata.cwd = started.thread.cwd.clone();
        metadata.cli_version = Some(started.thread.cli_version.clone());
        let inserted = state_db
            .insert_thread_if_absent(&metadata.build("openai"))
            .await
            .expect("insert_thread_if_absent should succeed");
        let updated = state_db
            .set_thread_mode(metadata.id, mode)
            .await
            .expect("set_thread_mode should succeed");
        assert!(
            inserted || updated,
            "thread metadata setup should insert or update an existing row"
        );
    }

    fn write_minimal_rollout(path: &Path, thread_id: &str, model_provider: &str) {
        let conversation_id = ThreadId::from_string(thread_id).expect("valid thread id");
        let timestamp = "2025-01-05T12:00:00Z";
        let meta = SessionMeta {
            id: conversation_id,
            forked_from_id: None,
            timestamp: timestamp.to_string(),
            cwd: std::env::temp_dir(),
            originator: "codex".to_string(),
            cli_version: "0.0.0-test".to_string(),
            source: ProtocolSessionSource::Cli,
            agent_path: None,
            agent_nickname: None,
            agent_role: None,
            model_provider: Some(model_provider.to_string()),
            base_instructions: None,
            dynamic_tools: None,
            memory_mode: None,
        };
        let payload = serde_json::to_value(SessionMetaLine { meta, git: None })
            .expect("session meta payload should serialize");
        let lines = [
            serde_json::json!({
                "timestamp": timestamp,
                "type": "session_meta",
                "payload": payload,
            })
            .to_string(),
            serde_json::json!({
                "timestamp": timestamp,
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "resident metadata update"}]
                }
            })
            .to_string(),
            serde_json::json!({
                "timestamp": timestamp,
                "type": "event_msg",
                "payload": {
                    "type": "user_message",
                    "message": "resident metadata update",
                    "kind": "plain"
                }
            })
            .to_string(),
        ];

        std::fs::create_dir_all(path.parent().expect("rollout path should have parent"))
            .expect("rollout directory should exist");
        std::fs::write(path, lines.join("\n") + "\n").expect("rollout should be writable");
    }

    async fn start_test_remote_server<F, Fut>(handler: F) -> String
    where
        F: FnOnce(tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>) -> Fut
            + Send
            + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        start_test_remote_server_with_auth(/*expected_auth_token*/ None, handler).await
    }

    async fn start_test_remote_server_with_auth<F, Fut>(
        expected_auth_token: Option<String>,
        handler: F,
    ) -> String
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
            let websocket = accept_hdr_async(
                stream,
                move |request: &WebSocketRequest, response: WebSocketResponse| {
                    let provided_auth_token = request
                        .headers()
                        .get(AUTHORIZATION)
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_owned);
                    let expected_auth_token = expected_auth_token
                        .as_ref()
                        .map(|token| format!("Bearer {token}"));
                    assert_eq!(provided_auth_token, expected_auth_token);
                    Ok(response)
                },
            )
            .await
            .expect("websocket upgrade should succeed");
            handler(websocket).await;
        });
        format!("ws://{addr}")
    }

    async fn expect_remote_initialize(
        websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    ) {
        let JSONRPCMessage::Request(request) = read_websocket_message(websocket).await else {
            panic!("expected initialize request");
        };
        assert_eq!(request.method, "initialize");
        write_websocket_message(
            websocket,
            JSONRPCMessage::Response(JSONRPCResponse {
                id: request.id,
                result: serde_json::json!({}),
            }),
        )
        .await;

        let JSONRPCMessage::Notification(notification) = read_websocket_message(websocket).await
        else {
            panic!("expected initialized notification");
        };
        assert_eq!(notification.method, "initialized");
    }

    async fn read_websocket_message(
        websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    ) -> JSONRPCMessage {
        loop {
            let frame = websocket
                .next()
                .await
                .expect("frame should be available")
                .expect("frame should decode");
            match frame {
                Message::Text(text) => {
                    return serde_json::from_str::<JSONRPCMessage>(&text)
                        .expect("text frame should be valid JSON-RPC");
                }
                Message::Binary(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {
                    continue;
                }
                Message::Close(_) => panic!("unexpected close frame"),
            }
        }
    }

    async fn write_websocket_message(
        websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
        message: JSONRPCMessage,
    ) {
        websocket
            .send(Message::Text(
                serde_json::to_string(&message)
                    .expect("message should serialize")
                    .into(),
            ))
            .await
            .expect("message should send");
    }

    fn command_execution_output_delta_notification(delta: &str) -> ServerNotification {
        ServerNotification::CommandExecutionOutputDelta(
            codex_app_server_protocol::CommandExecutionOutputDeltaNotification {
                thread_id: "thread".to_string(),
                turn_id: "turn".to_string(),
                item_id: "item".to_string(),
                delta: delta.to_string(),
            },
        )
    }

    fn agent_message_delta_notification(delta: &str) -> ServerNotification {
        ServerNotification::AgentMessageDelta(
            codex_app_server_protocol::AgentMessageDeltaNotification {
                thread_id: "thread".to_string(),
                turn_id: "turn".to_string(),
                item_id: "item".to_string(),
                delta: delta.to_string(),
            },
        )
    }

    fn item_completed_notification(text: &str) -> ServerNotification {
        ServerNotification::ItemCompleted(codex_app_server_protocol::ItemCompletedNotification {
            thread_id: "thread".to_string(),
            turn_id: "turn".to_string(),
            item: codex_app_server_protocol::ThreadItem::AgentMessage {
                id: "item".to_string(),
                text: text.to_string(),
                phase: None,
                memory_citation: None,
            },
        })
    }

    fn turn_completed_notification() -> ServerNotification {
        ServerNotification::TurnCompleted(codex_app_server_protocol::TurnCompletedNotification {
            thread_id: "thread".to_string(),
            turn: codex_app_server_protocol::Turn {
                id: "turn".to_string(),
                items: Vec::new(),
                status: codex_app_server_protocol::TurnStatus::Completed,
                error: None,
            },
        })
    }

    fn test_remote_connect_args(websocket_url: String) -> RemoteAppServerConnectArgs {
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
    async fn typed_request_roundtrip_works() {
        let client = start_test_client(SessionSource::Exec).await;
        let _response: ConfigRequirementsReadResponse = client
            .request_typed(ClientRequest::ConfigRequirementsRead {
                request_id: RequestId::Integer(1),
                params: None,
            })
            .await
            .expect("typed request should succeed");
        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn typed_request_reports_json_rpc_errors() {
        let client = start_test_client(SessionSource::Exec).await;
        let err = client
            .request_typed::<ConfigRequirementsReadResponse>(ClientRequest::ThreadRead {
                request_id: RequestId::Integer(99),
                params: codex_app_server_protocol::ThreadReadParams {
                    thread_id: "missing-thread".to_string(),
                    include_turns: false,
                },
            })
            .await
            .expect_err("missing thread should return a JSON-RPC error");
        assert!(
            err.to_string().starts_with("thread/read failed:"),
            "expected method-qualified JSON-RPC failure message"
        );
        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn caller_provided_session_source_is_applied() {
        for (session_source, expected_source) in [
            (SessionSource::Exec, ApiSessionSource::Exec),
            (SessionSource::Cli, ApiSessionSource::Cli),
        ] {
            let client = start_test_client(session_source).await;
            let parsed: ThreadStartResponse = client
                .request_typed(ClientRequest::ThreadStart {
                    request_id: RequestId::Integer(2),
                    params: ThreadStartParams {
                        ephemeral: Some(true),
                        ..ThreadStartParams::default()
                    },
                })
                .await
                .expect("thread/start should succeed");
            assert_eq!(parsed.thread.source, expected_source);
            client.shutdown().await.expect("shutdown should complete");
        }
    }

    #[tokio::test]
    async fn threads_started_via_app_server_are_visible_through_typed_requests() {
        let client = start_test_client(SessionSource::Cli).await;

        let response: ThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(3),
                params: ThreadStartParams {
                    ephemeral: Some(true),
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("thread/start should succeed");
        let read = client
            .request_typed::<codex_app_server_protocol::ThreadReadResponse>(
                ClientRequest::ThreadRead {
                    request_id: RequestId::Integer(4),
                    params: codex_app_server_protocol::ThreadReadParams {
                        thread_id: response.thread.id.clone(),
                        include_turns: false,
                    },
                },
            )
            .await
            .expect("thread/read should return the newly started thread");
        assert_eq!(read.thread.id, response.thread.id);

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn resident_thread_mode_is_preserved_through_typed_requests() {
        let client = start_test_client(SessionSource::Cli).await;

        let response: ThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(5),
                params: ThreadStartParams {
                    ephemeral: Some(true),
                    resident: true,
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("resident thread/start should succeed");
        assert_eq!(response.thread.mode, ThreadMode::ResidentAssistant);
        assert_eq!(response.thread.status, ThreadStatus::Idle);

        let read = client
            .request_typed::<codex_app_server_protocol::ThreadReadResponse>(
                ClientRequest::ThreadRead {
                    request_id: RequestId::Integer(6),
                    params: codex_app_server_protocol::ThreadReadParams {
                        thread_id: response.thread.id.clone(),
                        include_turns: false,
                    },
                },
            )
            .await
            .expect("thread/read should return the resident thread");
        assert_eq!(read.thread.id, response.thread.id);
        assert_eq!(read.thread.mode, ThreadMode::ResidentAssistant);
        assert_eq!(read.thread.status, ThreadStatus::Idle);

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn metadata_update_preserves_resident_thread_mode_through_typed_requests() {
        let client = start_test_client(SessionSource::Cli).await;
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let workspace = std::env::temp_dir().join(format!(
            "codex-app-server-client-metadata-workspace-{}-{unique_suffix}",
            std::process::id()
        ));
        std::fs::create_dir_all(&workspace).expect("workspace should exist");

        let started: ThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(7),
                params: ThreadStartParams {
                    resident: true,
                    cwd: Some(workspace.display().to_string()),
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("resident thread/start should succeed");
        let rollout_path = started
            .thread
            .path
            .clone()
            .expect("resident thread/start should expose rollout path");
        write_minimal_rollout(
            rollout_path.as_path(),
            &started.thread.id,
            &started.thread.model_provider,
        );

        let updated: ThreadMetadataUpdateResponse = client
            .request_typed(ClientRequest::ThreadMetadataUpdate {
                request_id: RequestId::Integer(8),
                params: ThreadMetadataUpdateParams {
                    thread_id: started.thread.id.clone(),
                    git_info: Some(ThreadMetadataGitInfoUpdateParams {
                        sha: Some(Some("abc123".to_string())),
                        branch: Some(Some("main".to_string())),
                        origin_url: None,
                    }),
                },
            })
            .await
            .expect("thread/metadata/update should succeed");

        assert_eq!(updated.thread.id, started.thread.id);
        assert_eq!(updated.thread.mode, ThreadMode::ResidentAssistant);
        assert_eq!(updated.thread.status, ThreadStatus::Idle);
        assert_eq!(
            updated.thread.git_info,
            Some(GitInfo {
                sha: Some("abc123".to_string()),
                branch: Some("main".to_string()),
                origin_url: started
                    .thread
                    .git_info
                    .as_ref()
                    .and_then(|git_info| git_info.origin_url.clone()),
            })
        );

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn loaded_read_preserves_resident_thread_mode_through_typed_requests() {
        let client = start_test_client(SessionSource::Cli).await;

        let started: ThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(9),
                params: ThreadStartParams {
                    resident: true,
                    ephemeral: Some(true),
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("resident thread/start should succeed");

        let loaded: ThreadLoadedReadResponse = client
            .request_typed(ClientRequest::ThreadLoadedRead {
                request_id: RequestId::Integer(10),
                params: ThreadLoadedReadParams::default(),
            })
            .await
            .expect("thread/loaded/read should succeed");

        let loaded_thread = loaded
            .data
            .iter()
            .find(|thread| thread.id == started.thread.id)
            .expect("loaded thread list should include the started resident thread");
        assert_eq!(loaded_thread.mode, ThreadMode::ResidentAssistant);
        assert_eq!(loaded_thread.status, ThreadStatus::Idle);
        assert!(loaded_thread.resident);

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn loaded_read_preserves_next_cursor_and_resident_mode_across_pages() {
        let client = start_test_client(SessionSource::Cli).await;

        let started_a: ThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(110),
                params: ThreadStartParams {
                    resident: true,
                    ephemeral: Some(true),
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("first resident thread/start should succeed");
        let started_b: ThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(111),
                params: ThreadStartParams {
                    resident: true,
                    ephemeral: Some(true),
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("second resident thread/start should succeed");

        let first_page: ThreadLoadedReadResponse = client
            .request_typed(ClientRequest::ThreadLoadedRead {
                request_id: RequestId::Integer(112),
                params: ThreadLoadedReadParams {
                    cursor: None,
                    limit: Some(1),
                    model_providers: None,
                    source_kinds: None,
                    cwd: None,
                },
            })
            .await
            .expect("first thread/loaded/read page should succeed");

        assert_eq!(first_page.data.len(), 1);
        assert_eq!(first_page.data[0].mode, ThreadMode::ResidentAssistant);
        assert_eq!(first_page.data[0].status, ThreadStatus::Idle);
        assert!(first_page.data[0].resident);
        let next_cursor = first_page
            .next_cursor
            .clone()
            .expect("first thread/loaded/read page should include next_cursor");

        let second_page: ThreadLoadedReadResponse = client
            .request_typed(ClientRequest::ThreadLoadedRead {
                request_id: RequestId::Integer(113),
                params: ThreadLoadedReadParams {
                    cursor: Some(next_cursor),
                    limit: Some(1),
                    model_providers: None,
                    source_kinds: None,
                    cwd: None,
                },
            })
            .await
            .expect("second thread/loaded/read page should succeed");

        assert_eq!(second_page.data.len(), 1);
        assert_eq!(second_page.data[0].mode, ThreadMode::ResidentAssistant);
        assert_eq!(second_page.data[0].status, ThreadStatus::Idle);
        assert!(second_page.data[0].resident);

        let page_ids = [
            first_page.data[0].id.clone(),
            second_page.data[0].id.clone(),
        ];
        assert!(page_ids.contains(&started_a.thread.id));
        assert!(page_ids.contains(&started_b.thread.id));

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn loaded_list_returns_loaded_thread_ids_through_typed_requests() {
        let client = start_test_client(SessionSource::Cli).await;

        let started: ThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(118),
                params: ThreadStartParams {
                    resident: true,
                    ephemeral: Some(true),
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("resident thread/start should succeed");

        let loaded: ThreadLoadedListResponse = client
            .request_typed(ClientRequest::ThreadLoadedList {
                request_id: RequestId::Integer(119),
                params: ThreadLoadedListParams::default(),
            })
            .await
            .expect("thread/loaded/list should succeed");

        assert!(loaded.data.contains(&started.thread.id));
        assert_eq!(loaded.next_cursor, None);

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn loaded_list_preserves_next_cursor_and_loaded_ids_across_pages() {
        let client = start_test_client(SessionSource::Cli).await;

        let started_a: ThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(120),
                params: ThreadStartParams {
                    resident: true,
                    ephemeral: Some(true),
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("first resident thread/start should succeed");
        let started_b: ThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(121),
                params: ThreadStartParams {
                    resident: true,
                    ephemeral: Some(true),
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("second resident thread/start should succeed");

        let first_page: ThreadLoadedListResponse = client
            .request_typed(ClientRequest::ThreadLoadedList {
                request_id: RequestId::Integer(122),
                params: ThreadLoadedListParams {
                    cursor: None,
                    limit: Some(1),
                    model_providers: None,
                    source_kinds: None,
                    cwd: None,
                },
            })
            .await
            .expect("first thread/loaded/list page should succeed");

        assert_eq!(first_page.data.len(), 1);
        let next_cursor = first_page
            .next_cursor
            .clone()
            .expect("first thread/loaded/list page should include next_cursor");

        let second_page: ThreadLoadedListResponse = client
            .request_typed(ClientRequest::ThreadLoadedList {
                request_id: RequestId::Integer(123),
                params: ThreadLoadedListParams {
                    cursor: Some(next_cursor),
                    limit: Some(1),
                    model_providers: None,
                    source_kinds: None,
                    cwd: None,
                },
            })
            .await
            .expect("second thread/loaded/list page should succeed");

        assert_eq!(second_page.data.len(), 1);

        let page_ids = [first_page.data[0].clone(), second_page.data[0].clone()];
        assert!(page_ids.contains(&started_a.thread.id));
        assert!(page_ids.contains(&started_b.thread.id));

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn list_preserves_resident_thread_mode_through_typed_requests() {
        let client = start_test_client(SessionSource::Cli).await;

        let started: ThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(11),
                params: ThreadStartParams {
                    resident: true,
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("resident thread/start should succeed");
        let rollout_path = started
            .thread
            .path
            .clone()
            .expect("resident thread/start should expose rollout path");
        write_minimal_rollout(
            rollout_path.as_path(),
            &started.thread.id,
            &started.thread.model_provider,
        );

        let listed: codex_app_server_protocol::ThreadListResponse = client
            .request_typed(ClientRequest::ThreadList {
                request_id: RequestId::Integer(12),
                params: codex_app_server_protocol::ThreadListParams {
                    cursor: None,
                    limit: Some(10),
                    sort_key: None,
                    model_providers: None,
                    source_kinds: None,
                    archived: None,
                    cwd: None,
                    search_term: None,
                },
            })
            .await
            .expect("thread/list should succeed");

        let listed_thread = listed
            .data
            .iter()
            .find(|thread| thread.id == started.thread.id)
            .expect("thread/list should include the started resident thread");
        assert_eq!(listed_thread.mode, ThreadMode::ResidentAssistant);
        assert_eq!(listed_thread.status, ThreadStatus::Idle);
        assert!(listed_thread.resident);

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn list_preserves_next_cursor_and_resident_mode_across_pages() {
        let client = start_test_client(SessionSource::Cli).await;

        let started_a: ThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(114),
                params: ThreadStartParams {
                    resident: true,
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("first resident thread/start should succeed");
        let rollout_path_a = started_a
            .thread
            .path
            .clone()
            .expect("first resident thread/start should expose rollout path");
        write_minimal_rollout(
            rollout_path_a.as_path(),
            &started_a.thread.id,
            &started_a.thread.model_provider,
        );

        let started_b: ThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(115),
                params: ThreadStartParams {
                    resident: true,
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("second resident thread/start should succeed");
        let rollout_path_b = started_b
            .thread
            .path
            .clone()
            .expect("second resident thread/start should expose rollout path");
        write_minimal_rollout(
            rollout_path_b.as_path(),
            &started_b.thread.id,
            &started_b.thread.model_provider,
        );

        let first_page: ThreadListResponse = client
            .request_typed(ClientRequest::ThreadList {
                request_id: RequestId::Integer(116),
                params: ThreadListParams {
                    cursor: None,
                    limit: Some(1),
                    sort_key: None,
                    model_providers: None,
                    source_kinds: None,
                    archived: None,
                    cwd: None,
                    search_term: None,
                },
            })
            .await
            .expect("first thread/list page should succeed");

        assert_eq!(first_page.data.len(), 1);
        assert_eq!(first_page.data[0].mode, ThreadMode::ResidentAssistant);
        assert_eq!(first_page.data[0].status, ThreadStatus::Idle);
        assert!(first_page.data[0].resident);
        let next_cursor = first_page
            .next_cursor
            .clone()
            .expect("first thread/list page should include next_cursor");

        let second_page: ThreadListResponse = client
            .request_typed(ClientRequest::ThreadList {
                request_id: RequestId::Integer(117),
                params: ThreadListParams {
                    cursor: Some(next_cursor),
                    limit: Some(1),
                    sort_key: None,
                    model_providers: None,
                    source_kinds: None,
                    archived: None,
                    cwd: None,
                    search_term: None,
                },
            })
            .await
            .expect("second thread/list page should succeed");

        assert_eq!(second_page.data.len(), 1);
        assert_eq!(second_page.data[0].mode, ThreadMode::ResidentAssistant);
        assert_eq!(second_page.data[0].status, ThreadStatus::Idle);
        assert!(second_page.data[0].resident);

        let page_ids = [
            first_page.data[0].id.clone(),
            second_page.data[0].id.clone(),
        ];
        assert!(page_ids.contains(&started_a.thread.id));
        assert!(page_ids.contains(&started_b.thread.id));

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn unarchive_preserves_resident_thread_mode_through_typed_requests() {
        let server = responses::start_mock_server().await;
        let _response = responses::mount_sse_once(
            &server,
            responses::sse(vec![
                responses::ev_response_created("resp-1"),
                responses::ev_assistant_message("msg-1", "Done"),
                responses::ev_completed("resp-1"),
            ]),
        )
        .await;

        let config = Arc::new(build_mock_responses_test_config(&server.uri()).await);
        mark_state_backfill_complete(config.as_ref()).await;
        let mut client = InProcessAppServerClient::start(InProcessClientStartArgs {
            arg0_paths: Arg0DispatchPaths::default(),
            config: config.clone(),
            cli_overrides: Vec::new(),
            loader_overrides: LoaderOverrides::default(),
            cloud_requirements: CloudRequirementsLoader::default(),
            feedback: CodexFeedback::new(),
            config_warnings: Vec::new(),
            session_source: SessionSource::Cli,
            enable_codex_api_key_env: false,
            client_name: "codex-app-server-client-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: DEFAULT_IN_PROCESS_CHANNEL_CAPACITY,
        })
        .await
        .expect("in-process app-server client should start");

        let started: ThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(13),
                params: ThreadStartParams {
                    resident: true,
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("resident thread/start should succeed");

        let _turn = client
            .request_typed::<codex_app_server_protocol::TurnStartResponse>(
                ClientRequest::TurnStart {
                    request_id: RequestId::Integer(14),
                    params: TurnStartParams {
                        thread_id: started.thread.id.clone(),
                        input: vec![UserInput::Text {
                            text: "materialize unarchived resident thread".to_string(),
                            text_elements: Vec::new(),
                        }],
                        ..TurnStartParams::default()
                    },
                },
            )
            .await
            .expect("turn/start should succeed");

        loop {
            let event = timeout(Duration::from_secs(5), client.next_event())
                .await
                .expect("turn completion event should arrive before timeout")
                .expect("event stream should stay open");
            if let InProcessServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                notification,
            )) = event
                && notification.thread_id == started.thread.id
                && notification.turn.status == TurnStatus::Completed
            {
                break;
            }
        }
        persist_thread_mode_in_state_db(&config, &started, "residentAssistant").await;

        let _: codex_app_server_protocol::ThreadArchiveResponse = client
            .request_typed(ClientRequest::ThreadArchive {
                request_id: RequestId::Integer(15),
                params: codex_app_server_protocol::ThreadArchiveParams {
                    thread_id: started.thread.id.clone(),
                },
            })
            .await
            .expect("thread/archive should succeed");

        let unarchived: codex_app_server_protocol::ThreadUnarchiveResponse = client
            .request_typed(ClientRequest::ThreadUnarchive {
                request_id: RequestId::Integer(16),
                params: codex_app_server_protocol::ThreadUnarchiveParams {
                    thread_id: started.thread.id.clone(),
                },
            })
            .await
            .expect("thread/unarchive should succeed");

        assert_eq!(unarchived.thread.id, started.thread.id);
        assert_eq!(unarchived.thread.mode, ThreadMode::ResidentAssistant);
        assert_eq!(unarchived.thread.status, ThreadStatus::NotLoaded);
        assert!(unarchived.thread.resident);

        let read: ThreadReadResponse = client
            .request_typed(ClientRequest::ThreadRead {
                request_id: RequestId::Integer(17),
                params: ThreadReadParams {
                    thread_id: started.thread.id.clone(),
                    include_turns: false,
                },
            })
            .await
            .expect("thread/read after unarchive should succeed");
        assert_eq!(read.thread.id, started.thread.id);
        assert_eq!(read.thread.mode, ThreadMode::ResidentAssistant);
        assert_eq!(read.thread.status, ThreadStatus::NotLoaded);
        assert!(read.thread.resident);

        let listed_thread = timeout(Duration::from_secs(5), async {
            loop {
                let listed: ThreadListResponse = client
                    .request_typed(ClientRequest::ThreadList {
                        request_id: RequestId::Integer(18),
                        params: ThreadListParams {
                            cursor: None,
                            limit: Some(10),
                            sort_key: None,
                            model_providers: Some(vec![started.thread.model_provider.clone()]),
                            source_kinds: None,
                            archived: Some(false),
                            cwd: None,
                            search_term: None,
                        },
                    })
                    .await
                    .expect("thread/list after unarchive should succeed");
                if let Some(listed_thread) = listed
                    .data
                    .into_iter()
                    .find(|thread| thread.id == started.thread.id)
                {
                    break listed_thread;
                }

                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("thread/list should eventually include the unarchived resident thread");
        assert_eq!(listed_thread.mode, ThreadMode::ResidentAssistant);
        assert_eq!(listed_thread.status, ThreadStatus::NotLoaded);
        assert!(listed_thread.resident);

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn archived_metadata_update_preserves_resident_thread_mode_through_typed_requests() {
        let server = responses::start_mock_server().await;
        let _response = responses::mount_sse_once(
            &server,
            responses::sse(vec![
                responses::ev_response_created("resp-1"),
                responses::ev_assistant_message("msg-1", "Done"),
                responses::ev_completed("resp-1"),
            ]),
        )
        .await;

        let config = Arc::new(build_mock_responses_test_config(&server.uri()).await);
        mark_state_backfill_complete(config.as_ref()).await;
        let mut client = InProcessAppServerClient::start(InProcessClientStartArgs {
            arg0_paths: Arg0DispatchPaths::default(),
            config: config.clone(),
            cli_overrides: Vec::new(),
            loader_overrides: LoaderOverrides::default(),
            cloud_requirements: CloudRequirementsLoader::default(),
            feedback: CodexFeedback::new(),
            config_warnings: Vec::new(),
            session_source: SessionSource::Cli,
            enable_codex_api_key_env: false,
            client_name: "codex-app-server-client-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: DEFAULT_IN_PROCESS_CHANNEL_CAPACITY,
        })
        .await
        .expect("in-process app-server client should start");

        let started: ThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(16),
                params: ThreadStartParams {
                    resident: true,
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("resident thread/start should succeed");

        let _turn = client
            .request_typed::<codex_app_server_protocol::TurnStartResponse>(
                ClientRequest::TurnStart {
                    request_id: RequestId::Integer(17),
                    params: TurnStartParams {
                        thread_id: started.thread.id.clone(),
                        input: vec![UserInput::Text {
                            text: "materialize archived resident thread".to_string(),
                            text_elements: Vec::new(),
                        }],
                        ..TurnStartParams::default()
                    },
                },
            )
            .await
            .expect("turn/start should succeed");

        loop {
            let event = timeout(Duration::from_secs(5), client.next_event())
                .await
                .expect("turn completion event should arrive before timeout")
                .expect("event stream should stay open");
            if let InProcessServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                notification,
            )) = event
                && notification.thread_id == started.thread.id
                && notification.turn.status == TurnStatus::Completed
            {
                break;
            }
        }
        persist_thread_mode_in_state_db(&config, &started, "residentAssistant").await;

        let _: codex_app_server_protocol::ThreadArchiveResponse = client
            .request_typed(ClientRequest::ThreadArchive {
                request_id: RequestId::Integer(18),
                params: codex_app_server_protocol::ThreadArchiveParams {
                    thread_id: started.thread.id.clone(),
                },
            })
            .await
            .expect("thread/archive should succeed");

        let updated: ThreadMetadataUpdateResponse = client
            .request_typed(ClientRequest::ThreadMetadataUpdate {
                request_id: RequestId::Integer(19),
                params: ThreadMetadataUpdateParams {
                    thread_id: started.thread.id.clone(),
                    git_info: Some(ThreadMetadataGitInfoUpdateParams {
                        sha: None,
                        branch: Some(Some("archived-main".to_string())),
                        origin_url: None,
                    }),
                },
            })
            .await
            .expect("thread/metadata/update should succeed for archived resident thread");

        assert_eq!(updated.thread.id, started.thread.id);
        assert_eq!(updated.thread.mode, ThreadMode::ResidentAssistant);
        assert_eq!(updated.thread.status, ThreadStatus::NotLoaded);
        assert_eq!(
            updated
                .thread
                .git_info
                .as_ref()
                .and_then(|git_info| git_info.branch.clone()),
            Some("archived-main".to_string())
        );

        let read = client
            .request_typed::<codex_app_server_protocol::ThreadReadResponse>(
                ClientRequest::ThreadRead {
                    request_id: RequestId::Integer(20),
                    params: codex_app_server_protocol::ThreadReadParams {
                        thread_id: started.thread.id.clone(),
                        include_turns: false,
                    },
                },
            )
            .await
            .expect("thread/read should return the archived resident thread");
        assert_eq!(read.thread.mode, ThreadMode::ResidentAssistant);
        assert_eq!(read.thread.status, ThreadStatus::NotLoaded);

        let listed_thread = timeout(Duration::from_secs(5), async {
            loop {
                let listed: codex_app_server_protocol::ThreadListResponse = client
                    .request_typed(ClientRequest::ThreadList {
                        request_id: RequestId::Integer(21),
                        params: codex_app_server_protocol::ThreadListParams {
                            cursor: None,
                            limit: Some(10),
                            sort_key: None,
                            model_providers: None,
                            source_kinds: None,
                            archived: Some(true),
                            cwd: None,
                            search_term: None,
                        },
                    })
                    .await
                    .expect("thread/list archived=true should return archived resident thread");
                if let Some(listed_thread) = listed
                    .data
                    .iter()
                    .find(|thread| thread.id == started.thread.id)
                {
                    return listed_thread.clone();
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("thread/list archived=true should converge before timeout");
        assert_eq!(listed_thread.mode, ThreadMode::ResidentAssistant);
        assert_eq!(listed_thread.status, ThreadStatus::NotLoaded);
        assert!(listed_thread.resident);

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn archived_read_preserves_resident_thread_mode_through_typed_requests() {
        let server = responses::start_mock_server().await;
        let _response = responses::mount_sse_once(
            &server,
            responses::sse(vec![
                responses::ev_response_created("resp-1"),
                responses::ev_assistant_message("msg-1", "Done"),
                responses::ev_completed("resp-1"),
            ]),
        )
        .await;

        let config = Arc::new(build_mock_responses_test_config(&server.uri()).await);
        mark_state_backfill_complete(config.as_ref()).await;
        let mut client = InProcessAppServerClient::start(InProcessClientStartArgs {
            arg0_paths: Arg0DispatchPaths::default(),
            config: config.clone(),
            cli_overrides: Vec::new(),
            loader_overrides: LoaderOverrides::default(),
            cloud_requirements: CloudRequirementsLoader::default(),
            feedback: CodexFeedback::new(),
            config_warnings: Vec::new(),
            session_source: SessionSource::Cli,
            enable_codex_api_key_env: false,
            client_name: "codex-app-server-client-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: DEFAULT_IN_PROCESS_CHANNEL_CAPACITY,
        })
        .await
        .expect("in-process app-server client should start");

        let started: ThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(20),
                params: ThreadStartParams {
                    resident: true,
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("resident thread/start should succeed");

        let _turn = client
            .request_typed::<codex_app_server_protocol::TurnStartResponse>(
                ClientRequest::TurnStart {
                    request_id: RequestId::Integer(21),
                    params: TurnStartParams {
                        thread_id: started.thread.id.clone(),
                        input: vec![UserInput::Text {
                            text: "materialize archived resident thread".to_string(),
                            text_elements: Vec::new(),
                        }],
                        ..TurnStartParams::default()
                    },
                },
            )
            .await
            .expect("turn/start should succeed");

        loop {
            let event = timeout(Duration::from_secs(5), client.next_event())
                .await
                .expect("turn completion event should arrive before timeout")
                .expect("event stream should stay open");
            if let InProcessServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                notification,
            )) = event
                && notification.thread_id == started.thread.id
                && notification.turn.status == TurnStatus::Completed
            {
                break;
            }
        }
        persist_thread_mode_in_state_db(&config, &started, "residentAssistant").await;

        let _: codex_app_server_protocol::ThreadArchiveResponse = client
            .request_typed(ClientRequest::ThreadArchive {
                request_id: RequestId::Integer(22),
                params: codex_app_server_protocol::ThreadArchiveParams {
                    thread_id: started.thread.id.clone(),
                },
            })
            .await
            .expect("thread/archive should succeed");

        let read = client
            .request_typed::<codex_app_server_protocol::ThreadReadResponse>(
                ClientRequest::ThreadRead {
                    request_id: RequestId::Integer(23),
                    params: codex_app_server_protocol::ThreadReadParams {
                        thread_id: started.thread.id.clone(),
                        include_turns: false,
                    },
                },
            )
            .await
            .expect("thread/read should return the archived resident thread");
        assert_eq!(read.thread.id, started.thread.id);
        assert_eq!(read.thread.mode, ThreadMode::ResidentAssistant);
        assert_eq!(read.thread.status, ThreadStatus::NotLoaded);
        assert!(read.thread.resident);

        let listed_thread = timeout(Duration::from_secs(5), async {
            loop {
                let listed: codex_app_server_protocol::ThreadListResponse = client
                    .request_typed(ClientRequest::ThreadList {
                        request_id: RequestId::Integer(24),
                        params: codex_app_server_protocol::ThreadListParams {
                            cursor: None,
                            limit: Some(10),
                            sort_key: None,
                            model_providers: None,
                            source_kinds: None,
                            archived: Some(true),
                            cwd: None,
                            search_term: None,
                        },
                    })
                    .await
                    .expect("thread/list archived=true should return archived resident thread");
                if let Some(listed_thread) = listed
                    .data
                    .iter()
                    .find(|thread| thread.id == started.thread.id)
                {
                    return listed_thread.clone();
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("thread/list archived=true should converge before timeout");
        assert_eq!(listed_thread.mode, ThreadMode::ResidentAssistant);
        assert_eq!(listed_thread.status, ThreadStatus::NotLoaded);
        assert!(listed_thread.resident);

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn resident_unsubscribe_preserves_mode_on_followup_reads_and_resume() {
        let client = start_test_client(SessionSource::Cli).await;

        let started: ThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(23),
                params: ThreadStartParams {
                    resident: true,
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("resident thread/start should succeed");
        let rollout_path = started
            .thread
            .path
            .clone()
            .expect("resident thread/start should expose rollout path");
        write_minimal_rollout(
            rollout_path.as_path(),
            &started.thread.id,
            &started.thread.model_provider,
        );

        let unsubscribed: codex_app_server_protocol::ThreadUnsubscribeResponse = client
            .request_typed(ClientRequest::ThreadUnsubscribe {
                request_id: RequestId::Integer(24),
                params: codex_app_server_protocol::ThreadUnsubscribeParams {
                    thread_id: started.thread.id.clone(),
                },
            })
            .await
            .expect("thread/unsubscribe should succeed");
        assert_eq!(unsubscribed.status, ThreadUnsubscribeStatus::Unsubscribed);

        let loaded_ids: ThreadLoadedListResponse = client
            .request_typed(ClientRequest::ThreadLoadedList {
                request_id: RequestId::Integer(241),
                params: ThreadLoadedListParams::default(),
            })
            .await
            .expect("thread/loaded/list should still include resident thread");
        assert!(
            loaded_ids.data.contains(&started.thread.id),
            "resident thread id should remain discoverable via thread/loaded/list after unsubscribe"
        );

        let loaded: ThreadLoadedReadResponse = client
            .request_typed(ClientRequest::ThreadLoadedRead {
                request_id: RequestId::Integer(25),
                params: ThreadLoadedReadParams::default(),
            })
            .await
            .expect("thread/loaded/read should still include resident thread");
        let loaded_thread = loaded
            .data
            .iter()
            .find(|thread| thread.id == started.thread.id)
            .expect("resident thread should stay loaded after unsubscribe");
        assert_eq!(loaded_thread.mode, ThreadMode::ResidentAssistant);
        assert!(loaded_thread.resident);
        assert_eq!(loaded_thread.status, ThreadStatus::Idle);

        let read = client
            .request_typed::<codex_app_server_protocol::ThreadReadResponse>(
                ClientRequest::ThreadRead {
                    request_id: RequestId::Integer(26),
                    params: codex_app_server_protocol::ThreadReadParams {
                        thread_id: started.thread.id.clone(),
                        include_turns: false,
                    },
                },
            )
            .await
            .expect("thread/read should preserve resident mode after unsubscribe");
        assert_eq!(read.thread.mode, ThreadMode::ResidentAssistant);
        assert!(read.thread.resident);
        assert_eq!(read.thread.status, ThreadStatus::Idle);

        let resumed: ThreadResumeResponse = client
            .request_typed(ClientRequest::ThreadResume {
                request_id: RequestId::Integer(27),
                params: codex_app_server_protocol::ThreadResumeParams {
                    thread_id: started.thread.id.clone(),
                    ..codex_app_server_protocol::ThreadResumeParams::default()
                },
            })
            .await
            .expect("thread/resume should reconnect to resident thread after unsubscribe");
        assert_eq!(resumed.thread.mode, ThreadMode::ResidentAssistant);
        assert!(resumed.thread.resident);
        assert_eq!(resumed.thread.status, ThreadStatus::Idle);

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn resident_workspace_changed_preserves_status_across_typed_reads_and_resume() {
        let client = start_test_client(SessionSource::Cli).await;
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let workspace = std::env::temp_dir().join(format!(
            "codex-app-server-client-workspace-{}-{unique_suffix}",
            std::process::id()
        ));
        std::fs::create_dir_all(&workspace).expect("workspace should exist");

        let started: ThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(28),
                params: ThreadStartParams {
                    resident: true,
                    cwd: Some(workspace.display().to_string()),
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("resident thread/start should succeed");
        let rollout_path = started
            .thread
            .path
            .clone()
            .expect("resident thread/start should expose rollout path");
        write_minimal_rollout(
            rollout_path.as_path(),
            &started.thread.id,
            &started.thread.model_provider,
        );

        std::fs::write(workspace.join("watched.txt"), "changed")
            .expect("workspace change should write");

        let changed_status = timeout(Duration::from_secs(5), async {
            loop {
                let read = client
                    .request_typed::<codex_app_server_protocol::ThreadReadResponse>(
                        ClientRequest::ThreadRead {
                            request_id: RequestId::Integer(29),
                            params: codex_app_server_protocol::ThreadReadParams {
                                thread_id: started.thread.id.clone(),
                                include_turns: false,
                            },
                        },
                    )
                    .await
                    .expect("thread/read should succeed while polling");
                if let ThreadStatus::Active { active_flags } = &read.thread.status
                    && active_flags.contains(&ThreadActiveFlag::WorkspaceChanged)
                {
                    return read.thread.status;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("workspace change should surface before timeout");

        assert_eq!(
            changed_status,
            ThreadStatus::Active {
                active_flags: vec![ThreadActiveFlag::WorkspaceChanged],
            }
        );

        let loaded: ThreadLoadedReadResponse = client
            .request_typed(ClientRequest::ThreadLoadedRead {
                request_id: RequestId::Integer(30),
                params: ThreadLoadedReadParams::default(),
            })
            .await
            .expect("thread/loaded/read should still include resident thread");
        let loaded_thread = loaded
            .data
            .iter()
            .find(|thread| thread.id == started.thread.id)
            .expect("thread/loaded/read should include the resident thread");
        assert_eq!(loaded_thread.status, changed_status);

        let resumed: ThreadResumeResponse = client
            .request_typed(ClientRequest::ThreadResume {
                request_id: RequestId::Integer(31),
                params: codex_app_server_protocol::ThreadResumeParams {
                    thread_id: started.thread.id.clone(),
                    ..codex_app_server_protocol::ThreadResumeParams::default()
                },
            })
            .await
            .expect("thread/resume should preserve resident workspace-changed status");
        assert_eq!(resumed.thread.status, changed_status);

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn resident_thread_events_split_mode_and_status_across_started_and_changed() {
        let mut client = start_test_client(SessionSource::Cli).await;
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let workspace = std::env::temp_dir().join(format!(
            "codex-app-server-client-events-{}-{unique_suffix}",
            std::process::id()
        ));
        std::fs::create_dir_all(&workspace).expect("workspace should exist");

        let started: ThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(35),
                params: ThreadStartParams {
                    resident: true,
                    cwd: Some(workspace.display().to_string()),
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("resident thread/start should succeed");
        let rollout_path = started
            .thread
            .path
            .clone()
            .expect("resident thread/start should expose rollout path");
        write_minimal_rollout(
            rollout_path.as_path(),
            &started.thread.id,
            &started.thread.model_provider,
        );

        let started_notification = timeout(Duration::from_secs(5), async {
            loop {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                if let InProcessServerEvent::ServerNotification(ServerNotification::ThreadStarted(
                    notification,
                )) = event
                    && notification.thread.id == started.thread.id
                {
                    break notification;
                }
            }
        })
        .await
        .expect("thread/started should arrive before timeout");
        assert_eq!(
            started_notification.thread.mode,
            ThreadMode::ResidentAssistant
        );
        assert_eq!(started_notification.thread.status, ThreadStatus::Idle);

        std::fs::write(workspace.join("watched.txt"), "changed")
            .expect("workspace change should write");

        let status_changed = timeout(Duration::from_secs(5), async {
            loop {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                if let InProcessServerEvent::ServerNotification(
                    ServerNotification::ThreadStatusChanged(notification),
                ) = event
                    && notification.thread_id == started.thread.id
                {
                    break notification;
                }
            }
        })
        .await
        .expect("thread/status/changed should arrive before timeout");
        assert_eq!(status_changed.thread_id, started.thread.id);
        assert_eq!(
            status_changed.status,
            ThreadStatus::Active {
                active_flags: vec![ThreadActiveFlag::WorkspaceChanged],
            }
        );

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn rollback_preserves_resident_thread_mode_through_typed_requests() {
        let server = responses::start_mock_server().await;
        let _response = responses::mount_sse_once(
            &server,
            responses::sse(vec![
                responses::ev_response_created("resp-1"),
                responses::ev_assistant_message("msg-1", "Done"),
                responses::ev_completed("resp-1"),
            ]),
        )
        .await;

        let config = Arc::new(build_mock_responses_test_config(&server.uri()).await);
        let mut client = InProcessAppServerClient::start(InProcessClientStartArgs {
            arg0_paths: Arg0DispatchPaths::default(),
            config: config.clone(),
            cli_overrides: Vec::new(),
            loader_overrides: LoaderOverrides::default(),
            cloud_requirements: CloudRequirementsLoader::default(),
            feedback: CodexFeedback::new(),
            config_warnings: Vec::new(),
            session_source: SessionSource::Cli,
            enable_codex_api_key_env: false,
            client_name: "codex-app-server-client-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: DEFAULT_IN_PROCESS_CHANNEL_CAPACITY,
        })
        .await
        .expect("in-process app-server client should start");

        let started: ThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(32),
                params: ThreadStartParams {
                    resident: true,
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("resident thread/start should succeed");
        persist_thread_mode_in_state_db(&config, &started, "residentAssistant").await;

        let _turn = client
            .request_typed::<codex_app_server_protocol::TurnStartResponse>(
                ClientRequest::TurnStart {
                    request_id: RequestId::Integer(33),
                    params: TurnStartParams {
                        thread_id: started.thread.id.clone(),
                        input: vec![UserInput::Text {
                            text: "rollback resident turn".to_string(),
                            text_elements: Vec::new(),
                        }],
                        ..TurnStartParams::default()
                    },
                },
            )
            .await
            .expect("turn/start should succeed");

        loop {
            let event = timeout(Duration::from_secs(5), client.next_event())
                .await
                .expect("turn completion event should arrive before timeout")
                .expect("event stream should stay open");
            if let InProcessServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                notification,
            )) = event
                && notification.thread_id == started.thread.id
                && notification.turn.status == TurnStatus::Completed
            {
                break;
            }
        }

        let rollback: ThreadRollbackResponse = client
            .request_typed(ClientRequest::ThreadRollback {
                request_id: RequestId::Integer(34),
                params: codex_app_server_protocol::ThreadRollbackParams {
                    thread_id: started.thread.id.clone(),
                    num_turns: 1,
                },
            })
            .await
            .expect("thread/rollback should succeed");
        assert_eq!(rollback.thread.id, started.thread.id);
        assert_eq!(rollback.thread.mode, ThreadMode::ResidentAssistant);
        assert_eq!(rollback.thread.status, ThreadStatus::Idle);
        assert!(rollback.thread.resident);

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn tiny_channel_capacity_still_supports_request_roundtrip() {
        let client =
            start_test_client_with_capacity(SessionSource::Exec, /*channel_capacity*/ 1).await;
        let _response: ConfigRequirementsReadResponse = client
            .request_typed(ClientRequest::ConfigRequirementsRead {
                request_id: RequestId::Integer(1),
                params: None,
            })
            .await
            .expect("typed request should succeed");
        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn forward_in_process_event_preserves_transcript_notifications_under_backpressure() {
        let (event_tx, mut event_rx) = mpsc::channel(1);
        event_tx
            .send(InProcessServerEvent::ServerNotification(
                command_execution_output_delta_notification("stdout-1"),
            ))
            .await
            .expect("initial event should enqueue");

        let mut skipped_events = 0usize;
        let result = forward_in_process_event(
            &event_tx,
            &mut skipped_events,
            InProcessServerEvent::ServerNotification(command_execution_output_delta_notification(
                "stdout-2",
            )),
            |_| {},
        )
        .await;
        assert_eq!(result, ForwardEventResult::Continue);
        assert_eq!(skipped_events, 1);

        let receive_task = tokio::spawn(async move {
            let mut events = Vec::new();
            for _ in 0..5 {
                events.push(
                    timeout(Duration::from_secs(2), event_rx.recv())
                        .await
                        .expect("event should arrive before timeout")
                        .expect("event stream should stay open"),
                );
            }
            events
        });

        for notification in [
            agent_message_delta_notification("hello"),
            item_completed_notification("hello"),
            turn_completed_notification(),
        ] {
            let result = forward_in_process_event(
                &event_tx,
                &mut skipped_events,
                InProcessServerEvent::ServerNotification(notification),
                |_| {},
            )
            .await;
            assert_eq!(result, ForwardEventResult::Continue);
        }
        assert_eq!(skipped_events, 0);

        let events = receive_task
            .await
            .expect("receiver task should join successfully");
        assert!(matches!(
            &events[0],
            InProcessServerEvent::ServerNotification(
                ServerNotification::CommandExecutionOutputDelta(notification)
            ) if notification.delta == "stdout-1"
        ));
        assert!(matches!(
            &events[1],
            InProcessServerEvent::Lagged { skipped: 1 }
        ));
        assert!(matches!(
            &events[2],
            InProcessServerEvent::ServerNotification(ServerNotification::AgentMessageDelta(
                notification
            )) if notification.delta == "hello"
        ));
        assert!(matches!(
            &events[3],
            InProcessServerEvent::ServerNotification(ServerNotification::ItemCompleted(
                notification
            )) if matches!(
                &notification.item,
                codex_app_server_protocol::ThreadItem::AgentMessage { text, .. } if text == "hello"
            )
        ));
        assert!(matches!(
            &events[4],
            InProcessServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                notification
            )) if notification.turn.status == codex_app_server_protocol::TurnStatus::Completed
        ));
    }

    #[tokio::test]
    async fn remote_typed_request_roundtrip_works() {
        let websocket_url = start_test_remote_server(|mut websocket| async move {
            expect_remote_initialize(&mut websocket).await;
            let JSONRPCMessage::Request(request) = read_websocket_message(&mut websocket).await
            else {
                panic!("expected account/read request");
            };
            assert_eq!(request.method, "account/read");
            write_websocket_message(
                &mut websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id,
                    result: serde_json::to_value(GetAccountResponse {
                        account: None,
                        requires_openai_auth: false,
                    })
                    .expect("response should serialize"),
                }),
            )
            .await;
            websocket.close(None).await.expect("close should succeed");
        })
        .await;
        let client = RemoteAppServerClient::connect(test_remote_connect_args(websocket_url))
            .await
            .expect("remote client should connect");

        let response: GetAccountResponse = client
            .request_typed(ClientRequest::GetAccount {
                request_id: RequestId::Integer(1),
                params: codex_app_server_protocol::GetAccountParams {
                    refresh_token: false,
                },
            })
            .await
            .expect("typed request should succeed");
        assert_eq!(response.account, None);

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn remote_connect_includes_auth_header_when_configured() {
        let auth_token = "remote-bearer-token".to_string();
        let websocket_url = start_test_remote_server_with_auth(
            Some(auth_token.clone()),
            |mut websocket| async move {
                expect_remote_initialize(&mut websocket).await;
                websocket.close(None).await.expect("close should succeed");
            },
        )
        .await;
        let client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
            auth_token: Some(auth_token),
            ..test_remote_connect_args(websocket_url)
        })
        .await
        .expect("remote client should connect");

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn remote_connect_rejects_non_loopback_ws_when_auth_configured() {
        let result = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
            websocket_url: "ws://example.com:4500".to_string(),
            auth_token: Some("remote-bearer-token".to_string()),
            ..test_remote_connect_args("ws://127.0.0.1:1".to_string())
        })
        .await;
        let err = match result {
            Ok(_) => panic!("non-loopback ws should be rejected before connect"),
            Err(err) => err,
        };
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(
            err.to_string()
                .contains("remote auth tokens require `wss://` or loopback `ws://` URLs")
        );
    }

    #[test]
    fn remote_auth_token_transport_policy_allows_wss_and_loopback_ws() {
        assert!(crate::remote::websocket_url_supports_auth_token(
            &url::Url::parse("wss://example.com:443").expect("wss URL should parse")
        ));
        assert!(crate::remote::websocket_url_supports_auth_token(
            &url::Url::parse("ws://127.0.0.1:4500").expect("loopback ws URL should parse")
        ));
        assert!(!crate::remote::websocket_url_supports_auth_token(
            &url::Url::parse("ws://example.com:4500").expect("non-loopback ws URL should parse")
        ));
    }

    #[tokio::test]
    async fn remote_duplicate_request_id_keeps_original_waiter() {
        let (first_request_seen_tx, first_request_seen_rx) = tokio::sync::oneshot::channel();
        let websocket_url = start_test_remote_server(|mut websocket| async move {
            expect_remote_initialize(&mut websocket).await;
            let JSONRPCMessage::Request(request) = read_websocket_message(&mut websocket).await
            else {
                panic!("expected account/read request");
            };
            assert_eq!(request.method, "account/read");
            first_request_seen_tx
                .send(request.id.clone())
                .expect("request id should send");
            assert!(
                timeout(
                    Duration::from_millis(100),
                    read_websocket_message(&mut websocket)
                )
                .await
                .is_err(),
                "duplicate request should not be forwarded to the server"
            );
            write_websocket_message(
                &mut websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id,
                    result: serde_json::to_value(GetAccountResponse {
                        account: None,
                        requires_openai_auth: false,
                    })
                    .expect("response should serialize"),
                }),
            )
            .await;
            let _ = websocket.next().await;
        })
        .await;
        let client = RemoteAppServerClient::connect(test_remote_connect_args(websocket_url))
            .await
            .expect("remote client should connect");
        let first_request_handle = client.request_handle();
        let second_request_handle = first_request_handle.clone();

        let first_request = tokio::spawn(async move {
            first_request_handle
                .request_typed::<GetAccountResponse>(ClientRequest::GetAccount {
                    request_id: RequestId::Integer(1),
                    params: codex_app_server_protocol::GetAccountParams {
                        refresh_token: false,
                    },
                })
                .await
        });

        let first_request_id = first_request_seen_rx
            .await
            .expect("server should observe the first request");
        assert_eq!(first_request_id, RequestId::Integer(1));

        let second_err = second_request_handle
            .request_typed::<GetAccountResponse>(ClientRequest::GetAccount {
                request_id: RequestId::Integer(1),
                params: codex_app_server_protocol::GetAccountParams {
                    refresh_token: false,
                },
            })
            .await
            .expect_err("duplicate request id should be rejected");
        assert_eq!(
            second_err.to_string(),
            "account/read transport error: duplicate remote app-server request id `1`"
        );

        let first_response = first_request
            .await
            .expect("first request task should join")
            .expect("first request should succeed");
        assert_eq!(
            first_response,
            GetAccountResponse {
                account: None,
                requires_openai_auth: false,
            }
        );

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn remote_notifications_arrive_over_websocket() {
        let websocket_url = start_test_remote_server(|mut websocket| async move {
            expect_remote_initialize(&mut websocket).await;
            write_websocket_message(
                &mut websocket,
                JSONRPCMessage::Notification(
                    serde_json::from_value(
                        serde_json::to_value(ServerNotification::AccountUpdated(
                            AccountUpdatedNotification {
                                auth_mode: None,
                                plan_type: None,
                            },
                        ))
                        .expect("notification should serialize"),
                    )
                    .expect("notification should convert to JSON-RPC"),
                ),
            )
            .await;
        })
        .await;
        let mut client = RemoteAppServerClient::connect(test_remote_connect_args(websocket_url))
            .await
            .expect("remote client should connect");

        let event = client.next_event().await.expect("event should arrive");
        assert!(matches!(
            event,
            AppServerEvent::ServerNotification(ServerNotification::AccountUpdated(_))
        ));

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn remote_backpressure_preserves_transcript_notifications() {
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        let websocket_url = start_test_remote_server(|mut websocket| async move {
            expect_remote_initialize(&mut websocket).await;
            for notification in [
                command_execution_output_delta_notification("stdout-1"),
                command_execution_output_delta_notification("stdout-2"),
                agent_message_delta_notification("hello"),
                item_completed_notification("hello"),
                turn_completed_notification(),
            ] {
                write_websocket_message(
                    &mut websocket,
                    JSONRPCMessage::Notification(
                        serde_json::from_value(
                            serde_json::to_value(notification)
                                .expect("notification should serialize"),
                        )
                        .expect("notification should convert to JSON-RPC"),
                    ),
                )
                .await;
            }
            let _ = done_rx.await;
        })
        .await;
        let mut client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
            websocket_url,
            auth_token: None,
            client_name: "codex-app-server-client-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 1,
        })
        .await
        .expect("remote client should connect");

        let first_event = timeout(Duration::from_secs(2), client.next_event())
            .await
            .expect("first event should arrive before timeout")
            .expect("event stream should stay open");
        assert!(matches!(
            first_event,
            AppServerEvent::ServerNotification(ServerNotification::CommandExecutionOutputDelta(
                notification
            )) if notification.delta == "stdout-1"
        ));

        let mut remaining_events = Vec::new();
        for _ in 0..4 {
            remaining_events.push(
                timeout(Duration::from_secs(2), client.next_event())
                    .await
                    .expect("event should arrive before timeout")
                    .expect("event stream should stay open"),
            );
        }

        let mut transcript_event_names = Vec::new();
        for event in &remaining_events {
            match event {
                AppServerEvent::Lagged { skipped: 1 } => {}
                AppServerEvent::ServerNotification(
                    ServerNotification::CommandExecutionOutputDelta(notification),
                ) if notification.delta == "stdout-2" => {}
                AppServerEvent::ServerNotification(ServerNotification::AgentMessageDelta(
                    notification,
                )) if notification.delta == "hello" => {
                    transcript_event_names.push("agent_message_delta");
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemCompleted(
                    notification,
                )) if matches!(
                    &notification.item,
                    codex_app_server_protocol::ThreadItem::AgentMessage { text, .. } if text == "hello"
                ) =>
                {
                    transcript_event_names.push("item_completed");
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                    notification,
                )) if notification.turn.status
                    == codex_app_server_protocol::TurnStatus::Completed =>
                {
                    transcript_event_names.push("turn_completed");
                }
                _ => panic!("unexpected remaining event: {event:?}"),
            }
        }
        assert_eq!(
            transcript_event_names,
            vec!["agent_message_delta", "item_completed", "turn_completed"]
        );

        done_tx
            .send(())
            .expect("server completion signal should send");
        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn remote_server_request_resolution_roundtrip_works() {
        let websocket_url = start_test_remote_server(|mut websocket| async move {
            expect_remote_initialize(&mut websocket).await;
            let request_id = RequestId::String("srv-1".to_string());
            let server_request = JSONRPCRequest {
                id: request_id.clone(),
                method: "item/tool/requestUserInput".to_string(),
                params: Some(
                    serde_json::to_value(ToolRequestUserInputParams {
                        thread_id: "thread-1".to_string(),
                        turn_id: "turn-1".to_string(),
                        item_id: "call-1".to_string(),
                        questions: vec![ToolRequestUserInputQuestion {
                            id: "question-1".to_string(),
                            header: "Mode".to_string(),
                            question: "Pick one".to_string(),
                            is_other: false,
                            is_secret: false,
                            options: Some(vec![]),
                        }],
                    })
                    .expect("params should serialize"),
                ),
                trace: None,
            };
            write_websocket_message(&mut websocket, JSONRPCMessage::Request(server_request)).await;

            let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await
            else {
                panic!("expected server request response");
            };
            assert_eq!(response.id, request_id);
        })
        .await;
        let mut client = RemoteAppServerClient::connect(test_remote_connect_args(websocket_url))
            .await
            .expect("remote client should connect");

        let AppServerEvent::ServerRequest(request) = client
            .next_event()
            .await
            .expect("request event should arrive")
        else {
            panic!("expected server request event");
        };
        client
            .resolve_server_request(request.id().clone(), serde_json::json!({}))
            .await
            .expect("server request should resolve");

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn remote_server_request_received_during_initialize_is_delivered() {
        let websocket_url = start_test_remote_server(|mut websocket| async move {
            let JSONRPCMessage::Request(request) = read_websocket_message(&mut websocket).await
            else {
                panic!("expected initialize request");
            };
            assert_eq!(request.method, "initialize");

            let request_id = RequestId::String("srv-init".to_string());
            write_websocket_message(
                &mut websocket,
                JSONRPCMessage::Request(JSONRPCRequest {
                    id: request_id.clone(),
                    method: "item/tool/requestUserInput".to_string(),
                    params: Some(
                        serde_json::to_value(ToolRequestUserInputParams {
                            thread_id: "thread-1".to_string(),
                            turn_id: "turn-1".to_string(),
                            item_id: "call-1".to_string(),
                            questions: vec![ToolRequestUserInputQuestion {
                                id: "question-1".to_string(),
                                header: "Mode".to_string(),
                                question: "Pick one".to_string(),
                                is_other: false,
                                is_secret: false,
                                options: Some(vec![]),
                            }],
                        })
                        .expect("params should serialize"),
                    ),
                    trace: None,
                }),
            )
            .await;
            write_websocket_message(
                &mut websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id,
                    result: serde_json::json!({}),
                }),
            )
            .await;

            let JSONRPCMessage::Notification(notification) =
                read_websocket_message(&mut websocket).await
            else {
                panic!("expected initialized notification");
            };
            assert_eq!(notification.method, "initialized");

            let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await
            else {
                panic!("expected server request response");
            };
            assert_eq!(response.id, request_id);
        })
        .await;
        let mut client = RemoteAppServerClient::connect(test_remote_connect_args(websocket_url))
            .await
            .expect("remote client should connect");

        let AppServerEvent::ServerRequest(request) = client
            .next_event()
            .await
            .expect("request event should arrive")
        else {
            panic!("expected server request event");
        };
        client
            .resolve_server_request(request.id().clone(), serde_json::json!({}))
            .await
            .expect("server request should resolve");

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn remote_unknown_server_request_is_rejected() {
        let websocket_url = start_test_remote_server(|mut websocket| async move {
            expect_remote_initialize(&mut websocket).await;
            let request_id = RequestId::String("srv-unknown".to_string());
            write_websocket_message(
                &mut websocket,
                JSONRPCMessage::Request(JSONRPCRequest {
                    id: request_id.clone(),
                    method: "thread/unknown".to_string(),
                    params: None,
                    trace: None,
                }),
            )
            .await;

            let JSONRPCMessage::Error(response) = read_websocket_message(&mut websocket).await
            else {
                panic!("expected JSON-RPC error response");
            };
            assert_eq!(response.id, request_id);
            assert_eq!(response.error.code, -32601);
            assert_eq!(
                response.error.message,
                "unsupported remote app-server request `thread/unknown`"
            );
        })
        .await;
        let client = RemoteAppServerClient::connect(test_remote_connect_args(websocket_url))
            .await
            .expect("remote client should connect");

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn remote_disconnect_surfaces_as_event() {
        let websocket_url = start_test_remote_server(|mut websocket| async move {
            expect_remote_initialize(&mut websocket).await;
            websocket.close(None).await.expect("close should succeed");
        })
        .await;
        let mut client = RemoteAppServerClient::connect(test_remote_connect_args(websocket_url))
            .await
            .expect("remote client should connect");

        let event = client
            .next_event()
            .await
            .expect("disconnect event should arrive");
        assert!(matches!(event, AppServerEvent::Disconnected { .. }));
    }

    #[test]
    fn typed_request_error_exposes_sources() {
        let transport = TypedRequestError::Transport {
            method: "config/read".to_string(),
            source: IoError::new(ErrorKind::BrokenPipe, "closed"),
        };
        assert_eq!(std::error::Error::source(&transport).is_some(), true);

        let server = TypedRequestError::Server {
            method: "thread/read".to_string(),
            source: JSONRPCErrorError {
                code: -32603,
                data: None,
                message: "internal".to_string(),
            },
        };
        assert_eq!(std::error::Error::source(&server).is_some(), false);

        let deserialize = TypedRequestError::Deserialize {
            method: "thread/start".to_string(),
            source: serde_json::from_str::<u32>("\"nope\"")
                .expect_err("invalid integer should return deserialize error"),
        };
        assert_eq!(std::error::Error::source(&deserialize).is_some(), true);
    }

    #[tokio::test]
    async fn next_event_surfaces_lagged_markers() {
        let (command_tx, _command_rx) = mpsc::channel(1);
        let (event_tx, event_rx) = mpsc::channel(1);
        let worker_handle = tokio::spawn(async {});
        event_tx
            .send(InProcessServerEvent::Lagged { skipped: 3 })
            .await
            .expect("lagged marker should enqueue");
        drop(event_tx);

        let mut client = InProcessAppServerClient {
            command_tx,
            event_rx,
            worker_handle,
        };

        let event = timeout(Duration::from_secs(2), client.next_event())
            .await
            .expect("lagged marker should arrive before timeout");
        assert!(matches!(
            event,
            Some(InProcessServerEvent::Lagged { skipped: 3 })
        ));

        client.shutdown().await.expect("shutdown should complete");
    }

    #[test]
    fn event_requires_delivery_marks_transcript_and_terminal_events() {
        assert!(event_requires_delivery(
            &InProcessServerEvent::ServerNotification(
                codex_app_server_protocol::ServerNotification::TurnCompleted(
                    codex_app_server_protocol::TurnCompletedNotification {
                        thread_id: "thread".to_string(),
                        turn: codex_app_server_protocol::Turn {
                            id: "turn".to_string(),
                            items: Vec::new(),
                            status: codex_app_server_protocol::TurnStatus::Completed,
                            error: None,
                        },
                    }
                )
            )
        ));
        assert!(event_requires_delivery(
            &InProcessServerEvent::ServerNotification(
                codex_app_server_protocol::ServerNotification::AgentMessageDelta(
                    codex_app_server_protocol::AgentMessageDeltaNotification {
                        thread_id: "thread".to_string(),
                        turn_id: "turn".to_string(),
                        item_id: "item".to_string(),
                        delta: "hello".to_string(),
                    }
                )
            )
        ));
        assert!(event_requires_delivery(
            &InProcessServerEvent::ServerNotification(
                codex_app_server_protocol::ServerNotification::ItemCompleted(
                    codex_app_server_protocol::ItemCompletedNotification {
                        thread_id: "thread".to_string(),
                        turn_id: "turn".to_string(),
                        item: codex_app_server_protocol::ThreadItem::AgentMessage {
                            id: "item".to_string(),
                            text: "hello".to_string(),
                            phase: None,
                            memory_citation: None,
                        },
                    }
                )
            )
        ));
        assert!(!event_requires_delivery(&InProcessServerEvent::Lagged {
            skipped: 1
        }));
        assert!(!event_requires_delivery(
            &InProcessServerEvent::ServerNotification(
                codex_app_server_protocol::ServerNotification::CommandExecutionOutputDelta(
                    codex_app_server_protocol::CommandExecutionOutputDeltaNotification {
                        thread_id: "thread".to_string(),
                        turn_id: "turn".to_string(),
                        item_id: "item".to_string(),
                        delta: "stdout".to_string(),
                    }
                )
            )
        ));
    }

    #[tokio::test]
    async fn runtime_start_args_leave_manager_bootstrap_to_app_server() {
        let config = Arc::new(build_test_config().await);

        let runtime_args = InProcessClientStartArgs {
            arg0_paths: Arg0DispatchPaths::default(),
            config: config.clone(),
            cli_overrides: Vec::new(),
            loader_overrides: LoaderOverrides::default(),
            cloud_requirements: CloudRequirementsLoader::default(),
            feedback: CodexFeedback::new(),
            config_warnings: Vec::new(),
            session_source: SessionSource::Exec,
            enable_codex_api_key_env: false,
            client_name: "codex-app-server-client-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: DEFAULT_IN_PROCESS_CHANNEL_CAPACITY,
        }
        .into_runtime_start_args();

        assert_eq!(runtime_args.config, config);
    }

    #[tokio::test]
    async fn shutdown_completes_promptly_without_retained_managers() {
        let client = start_test_client(SessionSource::Cli).await;

        timeout(Duration::from_secs(1), client.shutdown())
            .await
            .expect("shutdown should not wait for the 5s fallback timeout")
            .expect("shutdown should complete");
    }
}
