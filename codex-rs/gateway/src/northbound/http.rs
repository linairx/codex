use std::sync::Arc;

use crate::admission::GatewayAdmissionController;
use crate::admission::enforce_admission;
use crate::api::CreateThreadRequest;
use crate::api::GatewayHealthResponse;
use crate::api::InterruptTurnResponse;
use crate::api::ListThreadsRequest;
use crate::api::ListThreadsResponse;
use crate::api::ResolveServerRequestRequest;
use crate::api::ResolveServerRequestResponse;
use crate::api::StartTurnRequest;
use crate::auth::GatewayAuth;
use crate::auth::require_auth;
use crate::error::GatewayError;
use crate::event::GatewayEvent;
use crate::northbound::v2::GatewayV2Timeouts;
use crate::observability::GatewayObservability;
use crate::observability::observe_http_request;
use crate::runtime::GatewayRuntime;
use crate::scope::GatewayRequestContext;
use crate::scope::GatewayScopeRegistry;
use crate::v2::GatewayV2SessionFactory;
use axum::Json;
use axum::Router;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::middleware;
use axum::response::Sse;
use axum::response::sse::Event;
use axum::response::sse::KeepAlive;
use axum::routing::any;
use axum::routing::get;
use axum::routing::post;
use serde::Serialize;
use std::convert::Infallible;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

#[derive(Clone)]
pub struct GatewayHttpState {
    runtime: Arc<dyn GatewayRuntime>,
}

impl GatewayHttpState {
    pub fn new(runtime: Arc<dyn GatewayRuntime>) -> Self {
        Self { runtime }
    }
}

#[derive(Debug, Clone, Default, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventStreamQuery {
    thread_id: Option<String>,
}

pub fn router(
    runtime: Arc<dyn GatewayRuntime>,
    auth: GatewayAuth,
    admission: GatewayAdmissionController,
) -> Router {
    router_with_observability(
        runtime,
        auth,
        admission,
        GatewayObservability::default(),
        Arc::new(GatewayScopeRegistry::default()),
        None,
        GatewayV2Timeouts::default(),
    )
}

pub fn router_with_observability(
    runtime: Arc<dyn GatewayRuntime>,
    auth: GatewayAuth,
    admission: GatewayAdmissionController,
    observability: GatewayObservability,
    scope_registry: Arc<GatewayScopeRegistry>,
    v2_session_factory: Option<Arc<GatewayV2SessionFactory>>,
    v2_timeouts: GatewayV2Timeouts,
) -> Router {
    let state = GatewayHttpState::new(runtime);
    let v1 = Router::new()
        .route("/events", get(stream_events))
        .route("/threads", get(list_threads).post(create_thread))
        .route("/threads/{thread_id}", get(read_thread))
        .route("/threads/{thread_id}/turns", post(start_turn))
        .route("/server-requests/respond", post(resolve_server_request))
        .route(
            "/threads/{thread_id}/turns/{turn_id}/interrupt",
            post(interrupt_turn),
        )
        .route_layer(middleware::from_fn_with_state(
            admission.clone(),
            enforce_admission,
        ))
        .route_layer(middleware::from_fn_with_state(auth.clone(), require_auth));

    Router::new()
        .route(
            "/",
            any(crate::northbound::v2::websocket_upgrade_handler).with_state(
                crate::northbound::v2::GatewayV2State {
                    auth,
                    admission,
                    observability: observability.clone(),
                    scope_registry,
                    session_factory: v2_session_factory,
                    timeouts: v2_timeouts,
                },
            ),
        )
        .route("/healthz", get(healthz))
        .nest("/v1", v1)
        .route("/v1", any(|| async { axum::http::StatusCode::NOT_FOUND }))
        .layer(middleware::from_fn_with_state(
            observability,
            observe_http_request,
        ))
        .with_state(state)
}

async fn healthz(State(state): State<GatewayHttpState>) -> Json<GatewayHealthResponse> {
    Json(state.runtime.health())
}

async fn create_thread(
    context: GatewayRequestContext,
    State(state): State<GatewayHttpState>,
    Json(request): Json<CreateThreadRequest>,
) -> Result<Json<crate::api::ThreadResponse>, GatewayError> {
    let response = state.runtime.create_thread(context, request).await?;
    Ok(Json(response))
}

async fn list_threads(
    context: GatewayRequestContext,
    State(state): State<GatewayHttpState>,
    Query(request): Query<ListThreadsRequest>,
) -> Result<Json<ListThreadsResponse>, GatewayError> {
    let response = state.runtime.list_threads(context, request).await?;
    Ok(Json(response))
}

async fn read_thread(
    context: GatewayRequestContext,
    State(state): State<GatewayHttpState>,
    Path(thread_id): Path<String>,
) -> Result<Json<crate::api::ThreadResponse>, GatewayError> {
    let response = state.runtime.read_thread(context, thread_id).await?;
    Ok(Json(response))
}

async fn start_turn(
    context: GatewayRequestContext,
    State(state): State<GatewayHttpState>,
    Path(thread_id): Path<String>,
    Json(request): Json<StartTurnRequest>,
) -> Result<Json<crate::api::TurnResponse>, GatewayError> {
    let response = state
        .runtime
        .start_turn(context, thread_id, request)
        .await?;
    Ok(Json(response))
}

async fn interrupt_turn(
    context: GatewayRequestContext,
    State(state): State<GatewayHttpState>,
    Path((thread_id, turn_id)): Path<(String, String)>,
) -> Result<Json<InterruptTurnResponse>, GatewayError> {
    let response = state
        .runtime
        .interrupt_turn(context, thread_id, turn_id)
        .await?;
    Ok(Json(response))
}

async fn resolve_server_request(
    context: GatewayRequestContext,
    State(state): State<GatewayHttpState>,
    Json(request): Json<ResolveServerRequestRequest>,
) -> Result<Json<ResolveServerRequestResponse>, GatewayError> {
    let response = state
        .runtime
        .resolve_server_request(context, request)
        .await?;
    Ok(Json(response))
}

async fn stream_events(
    context: GatewayRequestContext,
    State(state): State<GatewayHttpState>,
    Query(query): Query<EventStreamQuery>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let thread_id_filter = query.thread_id;
    let runtime = state.runtime.clone();
    let stream = BroadcastStream::new(runtime.subscribe()).filter_map(move |event| {
        let thread_id_filter = thread_id_filter.clone();
        let context = context.clone();
        let runtime = runtime.clone();
        map_event_to_sse(
            event,
            thread_id_filter.as_deref(),
            runtime.as_ref(),
            &context,
        )
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

fn map_event_to_sse(
    event: Result<GatewayEvent, BroadcastStreamRecvError>,
    thread_id_filter: Option<&str>,
    runtime: &dyn GatewayRuntime,
    context: &GatewayRequestContext,
) -> Option<Result<Event, Infallible>> {
    let gateway_event = match event {
        Ok(event) => event,
        Err(BroadcastStreamRecvError::Lagged(skipped)) => {
            GatewayEvent::lagged(usize::try_from(skipped).unwrap_or(usize::MAX))
        }
    };

    if let Some(thread_id_filter) = thread_id_filter
        && gateway_event.thread_id.as_deref() != Some(thread_id_filter)
    {
        return None;
    }

    if !runtime.event_visible_to(context, &gateway_event) {
        return None;
    }

    let payload = serde_json::to_string(&gateway_event).ok()?;
    Some(Ok(Event::default()
        .event(&gateway_event.method)
        .data(payload)))
}

#[cfg(test)]
mod tests {
    use super::map_event_to_sse;
    use super::router;
    use super::router_with_observability;
    use crate::admission::GatewayAdmissionConfig;
    use crate::admission::GatewayAdmissionController;
    use crate::api::CreateThreadRequest;
    use crate::api::GatewayExecutionMode;
    use crate::api::GatewayHealthResponse;
    use crate::api::GatewayHealthStatus;
    use crate::api::GatewayRemoteWorkerHealth;
    use crate::api::GatewayThread;
    use crate::api::GatewayThreadActiveFlag;
    use crate::api::GatewayThreadStatus;
    use crate::api::GatewayTurn;
    use crate::api::GatewayTurnStatus;
    use crate::api::GatewayV2CompatibilityMode;
    use crate::api::GatewayV2ConnectionHealth;
    use crate::api::GatewayV2TransportConfig;
    use crate::api::InterruptTurnResponse;
    use crate::api::ListThreadsRequest;
    use crate::api::ListThreadsResponse;
    use crate::api::ResolveServerRequestRequest;
    use crate::api::ResolveServerRequestResponse;
    use crate::api::StartTurnRequest;
    use crate::api::ThreadResponse;
    use crate::api::TurnResponse;
    use crate::auth::GatewayAuth;
    use crate::error::GatewayError;
    use crate::event::GatewayEvent;
    use crate::northbound::v2::GatewayV2Timeouts;
    use crate::observability::GatewayObservability;
    use crate::runtime::GatewayRuntime;
    use crate::scope::GatewayRequestContext;
    use crate::scope::GatewayScopeRegistry;
    use async_trait::async_trait;
    use axum::body::Body;
    use axum::body::to_bytes;
    use axum::http::Request;
    use axum::http::StatusCode;
    use opentelemetry_sdk::metrics::InMemoryMetricExporter;
    use opentelemetry_sdk::metrics::data::AggregatedMetrics;
    use opentelemetry_sdk::metrics::data::MetricData;
    use pretty_assertions::assert_eq;
    use std::sync::Arc;
    use tokio::sync::broadcast;
    use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
    use tower::ServiceExt;

    struct FakeRuntime {
        events: broadcast::Sender<GatewayEvent>,
    }

    impl Default for FakeRuntime {
        fn default() -> Self {
            let (events, _rx) = broadcast::channel(16);
            Self { events }
        }
    }

    fn default_v2_connections() -> GatewayV2ConnectionHealth {
        GatewayV2ConnectionHealth {
            active_connection_count: 0,
            active_connection_pending_server_request_count: 0,
            active_connection_answered_but_unresolved_server_request_count: 0,
            peak_active_connection_count: 0,
            total_connection_count: 0,
            last_connection_started_at: None,
            last_connection_completed_at: None,
            last_connection_duration_ms: None,
            last_connection_outcome: None,
            last_connection_detail: None,
            last_connection_pending_server_request_count: 0,
            last_connection_answered_but_unresolved_server_request_count: 0,
        }
    }

    #[async_trait]
    impl GatewayRuntime for FakeRuntime {
        async fn create_thread(
            &self,
            _context: GatewayRequestContext,
            request: CreateThreadRequest,
        ) -> Result<ThreadResponse, GatewayError> {
            Ok(ThreadResponse {
                thread: GatewayThread {
                    id: "thread-1".to_string(),
                    preview: request.cwd.unwrap_or_default(),
                    ephemeral: false,
                    model_provider: "openai".to_string(),
                    created_at: 1,
                    updated_at: 1,
                    status: GatewayThreadStatus::Active {
                        active_flags: vec![GatewayThreadActiveFlag::WaitingOnUserInput],
                    },
                },
            })
        }

        async fn list_threads(
            &self,
            _context: GatewayRequestContext,
            request: ListThreadsRequest,
        ) -> Result<ListThreadsResponse, GatewayError> {
            Ok(ListThreadsResponse {
                data: vec![GatewayThread {
                    id: "thread-1".to_string(),
                    preview: request.search_term.unwrap_or_else(|| "preview".to_string()),
                    ephemeral: false,
                    model_provider: "openai".to_string(),
                    created_at: 1,
                    updated_at: 2,
                    status: GatewayThreadStatus::Idle,
                }],
                next_cursor: Some("cursor-2".to_string()),
                backwards_cursor: Some("cursor-0".to_string()),
            })
        }

        async fn read_thread(
            &self,
            _context: GatewayRequestContext,
            thread_id: String,
        ) -> Result<ThreadResponse, GatewayError> {
            Ok(ThreadResponse {
                thread: GatewayThread {
                    id: thread_id,
                    preview: "preview".to_string(),
                    ephemeral: false,
                    model_provider: "openai".to_string(),
                    created_at: 1,
                    updated_at: 1,
                    status: GatewayThreadStatus::Idle,
                },
            })
        }

        async fn start_turn(
            &self,
            _context: GatewayRequestContext,
            _thread_id: String,
            request: StartTurnRequest,
        ) -> Result<TurnResponse, GatewayError> {
            Ok(TurnResponse {
                turn: GatewayTurn {
                    id: "turn-1".to_string(),
                    status: GatewayTurnStatus::InProgress,
                    started_at: Some(1),
                    completed_at: None,
                    duration_ms: None,
                    error_message: Some(request.input),
                },
            })
        }

        async fn interrupt_turn(
            &self,
            _context: GatewayRequestContext,
            _thread_id: String,
            _turn_id: String,
        ) -> Result<InterruptTurnResponse, GatewayError> {
            Ok(InterruptTurnResponse { status: "accepted" })
        }

        async fn resolve_server_request(
            &self,
            _context: GatewayRequestContext,
            _request: ResolveServerRequestRequest,
        ) -> Result<ResolveServerRequestResponse, GatewayError> {
            Ok(ResolveServerRequestResponse { status: "accepted" })
        }

        fn health(&self) -> GatewayHealthResponse {
            GatewayHealthResponse {
                status: GatewayHealthStatus::Ok,
                runtime_mode: "embedded".to_string(),
                execution_mode: GatewayExecutionMode::InProcess,
                v2_compatibility: GatewayV2CompatibilityMode::Embedded,
                v2_transport: GatewayV2TransportConfig {
                    initialize_timeout_seconds: 30,
                    client_send_timeout_seconds: 10,
                    reconnect_retry_backoff_seconds: 1,
                    max_pending_server_requests: 64,
                },
                v2_connections: default_v2_connections(),
                remote_workers: None,
            }
        }

        fn subscribe(&self) -> broadcast::Receiver<GatewayEvent> {
            self.events.subscribe()
        }

        fn event_visible_to(
            &self,
            _context: &GatewayRequestContext,
            _event: &GatewayEvent,
        ) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn health_route_returns_runtime_health_payload() {
        let app = router(
            Arc::new(FakeRuntime::default()),
            GatewayAuth::Disabled,
            GatewayAdmissionController::default(),
        );

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(
            String::from_utf8(body.to_vec()).expect("utf8"),
            r#"{"status":"ok","runtimeMode":"embedded","executionMode":"inProcess","v2Compatibility":"embedded","v2Transport":{"initializeTimeoutSeconds":30,"clientSendTimeoutSeconds":10,"reconnectRetryBackoffSeconds":1,"maxPendingServerRequests":64},"v2Connections":{"activeConnectionCount":0,"activeConnectionPendingServerRequestCount":0,"activeConnectionAnsweredButUnresolvedServerRequestCount":0,"peakActiveConnectionCount":0,"totalConnectionCount":0,"lastConnectionStartedAt":null,"lastConnectionCompletedAt":null,"lastConnectionDurationMs":null,"lastConnectionOutcome":null,"lastConnectionDetail":null,"lastConnectionPendingServerRequestCount":0,"lastConnectionAnsweredButUnresolvedServerRequestCount":0},"remoteWorkers":null}"#
        );
    }

    #[tokio::test]
    async fn health_route_serializes_remote_worker_retry_metadata_in_camel_case() {
        struct RemoteHealthRuntime;

        #[async_trait]
        impl GatewayRuntime for RemoteHealthRuntime {
            async fn create_thread(
                &self,
                _context: GatewayRequestContext,
                _request: CreateThreadRequest,
            ) -> Result<ThreadResponse, GatewayError> {
                unreachable!("health-only runtime should not receive create_thread")
            }

            async fn list_threads(
                &self,
                _context: GatewayRequestContext,
                _request: ListThreadsRequest,
            ) -> Result<ListThreadsResponse, GatewayError> {
                unreachable!("health-only runtime should not receive list_threads")
            }

            async fn read_thread(
                &self,
                _context: GatewayRequestContext,
                _thread_id: String,
            ) -> Result<ThreadResponse, GatewayError> {
                unreachable!("health-only runtime should not receive read_thread")
            }

            async fn start_turn(
                &self,
                _context: GatewayRequestContext,
                _thread_id: String,
                _request: StartTurnRequest,
            ) -> Result<TurnResponse, GatewayError> {
                unreachable!("health-only runtime should not receive start_turn")
            }

            async fn interrupt_turn(
                &self,
                _context: GatewayRequestContext,
                _thread_id: String,
                _turn_id: String,
            ) -> Result<InterruptTurnResponse, GatewayError> {
                unreachable!("health-only runtime should not receive interrupt_turn")
            }

            async fn resolve_server_request(
                &self,
                _context: GatewayRequestContext,
                _request: ResolveServerRequestRequest,
            ) -> Result<ResolveServerRequestResponse, GatewayError> {
                unreachable!("health-only runtime should not receive resolve_server_request")
            }

            fn health(&self) -> GatewayHealthResponse {
                GatewayHealthResponse {
                    status: GatewayHealthStatus::Degraded,
                    runtime_mode: "remote".to_string(),
                    execution_mode: GatewayExecutionMode::WorkerManaged,
                    v2_compatibility: GatewayV2CompatibilityMode::RemoteSingleWorker,
                    v2_transport: GatewayV2TransportConfig {
                        initialize_timeout_seconds: 30,
                        client_send_timeout_seconds: 10,
                        reconnect_retry_backoff_seconds: 1,
                        max_pending_server_requests: 64,
                    },
                    v2_connections: default_v2_connections(),
                    remote_workers: Some(vec![GatewayRemoteWorkerHealth {
                        worker_id: 0,
                        websocket_url: "ws://127.0.0.1:8081".to_string(),
                        healthy: false,
                        reconnecting: true,
                        reconnect_attempt_count: 2,
                        last_error: Some("remote app server event stream ended".to_string()),
                        last_state_change_at: Some(1710000000),
                        last_error_at: Some(1710000001),
                        next_reconnect_at: Some(1710000002),
                    }]),
                }
            }

            fn subscribe(&self) -> broadcast::Receiver<GatewayEvent> {
                let (events, _rx) = broadcast::channel(1);
                events.subscribe()
            }

            fn event_visible_to(
                &self,
                _context: &GatewayRequestContext,
                _event: &GatewayEvent,
            ) -> bool {
                true
            }
        }

        let app = router(
            Arc::new(RemoteHealthRuntime),
            GatewayAuth::Disabled,
            GatewayAdmissionController::default(),
        );

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(
            String::from_utf8(body.to_vec()).expect("utf8"),
            r#"{"status":"degraded","runtimeMode":"remote","executionMode":"workerManaged","v2Compatibility":"remoteSingleWorker","v2Transport":{"initializeTimeoutSeconds":30,"clientSendTimeoutSeconds":10,"reconnectRetryBackoffSeconds":1,"maxPendingServerRequests":64},"v2Connections":{"activeConnectionCount":0,"activeConnectionPendingServerRequestCount":0,"activeConnectionAnsweredButUnresolvedServerRequestCount":0,"peakActiveConnectionCount":0,"totalConnectionCount":0,"lastConnectionStartedAt":null,"lastConnectionCompletedAt":null,"lastConnectionDurationMs":null,"lastConnectionOutcome":null,"lastConnectionDetail":null,"lastConnectionPendingServerRequestCount":0,"lastConnectionAnsweredButUnresolvedServerRequestCount":0},"remoteWorkers":[{"workerId":0,"websocketUrl":"ws://127.0.0.1:8081","healthy":false,"reconnecting":true,"reconnectAttemptCount":2,"lastError":"remote app server event stream ended","lastStateChangeAt":1710000000,"lastErrorAt":1710000001,"nextReconnectAt":1710000002}]}"#
        );
    }

    #[tokio::test]
    async fn health_route_serializes_v2_client_send_timeout_connection_outcome() {
        struct RemoteTimeoutHealthRuntime;

        #[async_trait]
        impl GatewayRuntime for RemoteTimeoutHealthRuntime {
            async fn create_thread(
                &self,
                _context: GatewayRequestContext,
                _request: CreateThreadRequest,
            ) -> Result<ThreadResponse, GatewayError> {
                unreachable!("health-only runtime should not receive create_thread")
            }

            async fn list_threads(
                &self,
                _context: GatewayRequestContext,
                _request: ListThreadsRequest,
            ) -> Result<ListThreadsResponse, GatewayError> {
                unreachable!("health-only runtime should not receive list_threads")
            }

            async fn read_thread(
                &self,
                _context: GatewayRequestContext,
                _thread_id: String,
            ) -> Result<ThreadResponse, GatewayError> {
                unreachable!("health-only runtime should not receive read_thread")
            }

            async fn start_turn(
                &self,
                _context: GatewayRequestContext,
                _thread_id: String,
                _request: StartTurnRequest,
            ) -> Result<TurnResponse, GatewayError> {
                unreachable!("health-only runtime should not receive start_turn")
            }

            async fn interrupt_turn(
                &self,
                _context: GatewayRequestContext,
                _thread_id: String,
                _turn_id: String,
            ) -> Result<InterruptTurnResponse, GatewayError> {
                unreachable!("health-only runtime should not receive interrupt_turn")
            }

            async fn resolve_server_request(
                &self,
                _context: GatewayRequestContext,
                _request: ResolveServerRequestRequest,
            ) -> Result<ResolveServerRequestResponse, GatewayError> {
                unreachable!("health-only runtime should not receive resolve_server_request")
            }

            fn health(&self) -> GatewayHealthResponse {
                GatewayHealthResponse {
                    status: GatewayHealthStatus::Degraded,
                    runtime_mode: "remote".to_string(),
                    execution_mode: GatewayExecutionMode::WorkerManaged,
                    v2_compatibility: GatewayV2CompatibilityMode::RemoteSingleWorker,
                    v2_transport: GatewayV2TransportConfig {
                        initialize_timeout_seconds: 30,
                        client_send_timeout_seconds: 1,
                        reconnect_retry_backoff_seconds: 1,
                        max_pending_server_requests: 64,
                    },
                    v2_connections: GatewayV2ConnectionHealth {
                        active_connection_count: 0,
                        active_connection_pending_server_request_count: 0,
                        active_connection_answered_but_unresolved_server_request_count: 0,
                        peak_active_connection_count: 3,
                        total_connection_count: 7,
                        last_connection_started_at: Some(1710000001),
                        last_connection_completed_at: Some(1710000003),
                        last_connection_duration_ms: Some(2500),
                        last_connection_outcome: Some("client_send_timed_out".to_string()),
                        last_connection_detail: Some(
                            "gateway websocket send timed out".to_string(),
                        ),
                        last_connection_pending_server_request_count: 2,
                        last_connection_answered_but_unresolved_server_request_count: 1,
                    },
                    remote_workers: Some(vec![GatewayRemoteWorkerHealth {
                        worker_id: 0,
                        websocket_url: "ws://127.0.0.1:8081".to_string(),
                        healthy: true,
                        reconnecting: false,
                        reconnect_attempt_count: 0,
                        last_error: None,
                        last_state_change_at: Some(1710000000),
                        last_error_at: None,
                        next_reconnect_at: None,
                    }]),
                }
            }

            fn subscribe(&self) -> broadcast::Receiver<GatewayEvent> {
                let (events, _rx) = broadcast::channel(1);
                events.subscribe()
            }

            fn event_visible_to(
                &self,
                _context: &GatewayRequestContext,
                _event: &GatewayEvent,
            ) -> bool {
                true
            }
        }

        let app = router(
            Arc::new(RemoteTimeoutHealthRuntime),
            GatewayAuth::Disabled,
            GatewayAdmissionController::default(),
        );

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(
            String::from_utf8(body.to_vec()).expect("utf8"),
            r#"{"status":"degraded","runtimeMode":"remote","executionMode":"workerManaged","v2Compatibility":"remoteSingleWorker","v2Transport":{"initializeTimeoutSeconds":30,"clientSendTimeoutSeconds":1,"reconnectRetryBackoffSeconds":1,"maxPendingServerRequests":64},"v2Connections":{"activeConnectionCount":0,"activeConnectionPendingServerRequestCount":0,"activeConnectionAnsweredButUnresolvedServerRequestCount":0,"peakActiveConnectionCount":3,"totalConnectionCount":7,"lastConnectionStartedAt":1710000001,"lastConnectionCompletedAt":1710000003,"lastConnectionDurationMs":2500,"lastConnectionOutcome":"client_send_timed_out","lastConnectionDetail":"gateway websocket send timed out","lastConnectionPendingServerRequestCount":2,"lastConnectionAnsweredButUnresolvedServerRequestCount":1},"remoteWorkers":[{"workerId":0,"websocketUrl":"ws://127.0.0.1:8081","healthy":true,"reconnecting":false,"reconnectAttemptCount":0,"lastError":null,"lastStateChangeAt":1710000000,"lastErrorAt":null,"nextReconnectAt":null}]}"#
        );
    }

    #[tokio::test]
    async fn create_thread_route_returns_thread_payload() {
        let app = router(
            Arc::new(FakeRuntime::default()),
            GatewayAuth::Disabled,
            GatewayAdmissionController::default(),
        );

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/threads")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&CreateThreadRequest {
                            cwd: Some("/tmp/project".to_string()),
                            model: None,
                            ephemeral: None,
                        })
                        .expect("request body"),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(
            String::from_utf8(body.to_vec()).expect("utf8"),
            r#"{"thread":{"id":"thread-1","preview":"/tmp/project","ephemeral":false,"modelProvider":"openai","createdAt":1,"updatedAt":1,"status":{"type":"active","activeFlags":["waitingOnUserInput"]}}}"#
        );
    }

    #[tokio::test]
    async fn list_threads_route_returns_page_payload() {
        let app = router(
            Arc::new(FakeRuntime::default()),
            GatewayAuth::Disabled,
            GatewayAdmissionController::default(),
        );

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/threads?limit=20&sortKey=updatedAt&sortDirection=desc&searchTerm=gateway")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(
            String::from_utf8(body.to_vec()).expect("utf8"),
            r#"{"data":[{"id":"thread-1","preview":"gateway","ephemeral":false,"modelProvider":"openai","createdAt":1,"updatedAt":2,"status":{"type":"idle"}}],"nextCursor":"cursor-2","backwardsCursor":"cursor-0"}"#
        );
    }

    #[tokio::test]
    async fn read_thread_route_returns_thread_payload() {
        let app = router(
            Arc::new(FakeRuntime::default()),
            GatewayAuth::Disabled,
            GatewayAdmissionController::default(),
        );

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/threads/thread-123")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(
            String::from_utf8(body.to_vec()).expect("utf8"),
            r#"{"thread":{"id":"thread-123","preview":"preview","ephemeral":false,"modelProvider":"openai","createdAt":1,"updatedAt":1,"status":{"type":"idle"}}}"#
        );
    }

    #[tokio::test]
    async fn start_turn_route_returns_turn_payload() {
        let app = router(
            Arc::new(FakeRuntime::default()),
            GatewayAuth::Disabled,
            GatewayAdmissionController::default(),
        );

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/threads/thread-123/turns")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&StartTurnRequest {
                            input: "hello".to_string(),
                        })
                        .expect("request body"),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(
            String::from_utf8(body.to_vec()).expect("utf8"),
            r#"{"turn":{"id":"turn-1","status":"inProgress","startedAt":1,"completedAt":null,"durationMs":null,"errorMessage":"hello"}}"#
        );
    }

    #[tokio::test]
    async fn interrupt_turn_route_returns_accepted_payload() {
        let app = router(
            Arc::new(FakeRuntime::default()),
            GatewayAuth::Disabled,
            GatewayAdmissionController::default(),
        );

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/threads/thread-123/turns/turn-456/interrupt")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(
            String::from_utf8(body.to_vec()).expect("utf8"),
            r#"{"status":"accepted"}"#
        );
    }

    #[tokio::test]
    async fn resolve_server_request_route_returns_accepted_payload() {
        let app = router(
            Arc::new(FakeRuntime::default()),
            GatewayAuth::Disabled,
            GatewayAdmissionController::default(),
        );

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/server-requests/respond")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"requestId":"req-1","type":"toolRequestUserInput","answers":{"confirm":{"answers":["Yes"]}}}"#,
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(
            String::from_utf8(body.to_vec()).expect("utf8"),
            r#"{"status":"accepted"}"#
        );
    }

    #[test]
    fn sse_event_filter_keeps_matching_thread() {
        let event = map_event_to_sse(
            Ok(GatewayEvent {
                method: "turn/completed".to_string(),
                thread_id: Some("thread-123".to_string()),
                data: serde_json::json!({ "turnId": "turn-1" }),
            }),
            Some("thread-123"),
            &FakeRuntime::default(),
            &GatewayRequestContext::default(),
        );

        assert_eq!(event.is_some(), true);
    }

    #[test]
    fn sse_event_filter_drops_non_matching_thread() {
        let event = map_event_to_sse(
            Ok(GatewayEvent {
                method: "turn/completed".to_string(),
                thread_id: Some("thread-123".to_string()),
                data: serde_json::json!({ "turnId": "turn-1" }),
            }),
            Some("thread-999"),
            &FakeRuntime::default(),
            &GatewayRequestContext::default(),
        );

        assert!(event.is_none());
    }

    #[test]
    fn sse_event_converts_broadcast_lag_to_gateway_lag_event() {
        let event = map_event_to_sse(
            Err(BroadcastStreamRecvError::Lagged(3)),
            None,
            &FakeRuntime::default(),
            &GatewayRequestContext::default(),
        );

        assert_eq!(event.is_some(), true);
    }

    #[test]
    fn sse_event_filter_drops_events_outside_request_scope() {
        #[derive(Default)]
        struct HiddenRuntime;

        #[async_trait]
        impl GatewayRuntime for HiddenRuntime {
            async fn create_thread(
                &self,
                _context: GatewayRequestContext,
                _request: CreateThreadRequest,
            ) -> Result<ThreadResponse, GatewayError> {
                unreachable!("not used")
            }

            async fn list_threads(
                &self,
                _context: GatewayRequestContext,
                _request: ListThreadsRequest,
            ) -> Result<ListThreadsResponse, GatewayError> {
                unreachable!("not used")
            }

            async fn read_thread(
                &self,
                _context: GatewayRequestContext,
                _thread_id: String,
            ) -> Result<ThreadResponse, GatewayError> {
                unreachable!("not used")
            }

            async fn start_turn(
                &self,
                _context: GatewayRequestContext,
                _thread_id: String,
                _request: StartTurnRequest,
            ) -> Result<TurnResponse, GatewayError> {
                unreachable!("not used")
            }

            async fn interrupt_turn(
                &self,
                _context: GatewayRequestContext,
                _thread_id: String,
                _turn_id: String,
            ) -> Result<InterruptTurnResponse, GatewayError> {
                unreachable!("not used")
            }

            async fn resolve_server_request(
                &self,
                _context: GatewayRequestContext,
                _request: ResolveServerRequestRequest,
            ) -> Result<ResolveServerRequestResponse, GatewayError> {
                unreachable!("not used")
            }

            fn health(&self) -> GatewayHealthResponse {
                GatewayHealthResponse {
                    status: GatewayHealthStatus::Ok,
                    runtime_mode: "embedded".to_string(),
                    execution_mode: GatewayExecutionMode::InProcess,
                    v2_compatibility: GatewayV2CompatibilityMode::Embedded,
                    v2_transport: GatewayV2TransportConfig {
                        initialize_timeout_seconds: 30,
                        client_send_timeout_seconds: 10,
                        reconnect_retry_backoff_seconds: 1,
                        max_pending_server_requests: 64,
                    },
                    v2_connections: default_v2_connections(),
                    remote_workers: None,
                }
            }

            fn subscribe(&self) -> broadcast::Receiver<GatewayEvent> {
                let (events, _rx) = broadcast::channel(1);
                events.subscribe()
            }

            fn event_visible_to(
                &self,
                _context: &GatewayRequestContext,
                _event: &GatewayEvent,
            ) -> bool {
                false
            }
        }

        let event = map_event_to_sse(
            Ok(GatewayEvent {
                method: "turn/completed".to_string(),
                thread_id: Some("thread-123".to_string()),
                data: serde_json::json!({ "turnId": "turn-1" }),
            }),
            Some("thread-123"),
            &HiddenRuntime,
            &GatewayRequestContext {
                tenant_id: "tenant-a".to_string(),
                project_id: Some("project-a".to_string()),
            },
        );

        assert!(event.is_none());
    }

    #[tokio::test]
    async fn v1_routes_require_bearer_token_when_configured() {
        let app = router(
            Arc::new(FakeRuntime::default()),
            GatewayAuth::BearerToken {
                token: "secret-token".to_string(),
            },
            GatewayAdmissionController::default(),
        );

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/threads")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"cwd":"/tmp/project"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(create_response.status(), StatusCode::UNAUTHORIZED);

        let authorized_response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/threads")
                    .header("content-type", "application/json")
                    .header("authorization", "Bearer secret-token")
                    .body(Body::from(r#"{"cwd":"/tmp/project"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(authorized_response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn healthz_route_stays_public_when_auth_is_enabled() {
        let app = router(
            Arc::new(FakeRuntime::default()),
            GatewayAuth::BearerToken {
                token: "secret-token".to_string(),
            },
            GatewayAdmissionController::default(),
        );

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn runtime_errors_map_to_http_status_codes() {
        #[derive(Default)]
        struct ErrorRuntime;

        #[async_trait]
        impl GatewayRuntime for ErrorRuntime {
            async fn create_thread(
                &self,
                _context: GatewayRequestContext,
                _request: CreateThreadRequest,
            ) -> Result<ThreadResponse, GatewayError> {
                Err(GatewayError::InvalidRequest("bad create".to_string()))
            }

            async fn list_threads(
                &self,
                _context: GatewayRequestContext,
                _request: ListThreadsRequest,
            ) -> Result<ListThreadsResponse, GatewayError> {
                Err(GatewayError::InvalidRequest("bad list".to_string()))
            }

            async fn read_thread(
                &self,
                _context: GatewayRequestContext,
                _thread_id: String,
            ) -> Result<ThreadResponse, GatewayError> {
                Err(GatewayError::NotFound(
                    "thread not found: thread-404".to_string(),
                ))
            }

            async fn start_turn(
                &self,
                _context: GatewayRequestContext,
                _thread_id: String,
                _request: StartTurnRequest,
            ) -> Result<TurnResponse, GatewayError> {
                Err(GatewayError::Upstream("upstream unavailable".to_string()))
            }

            async fn interrupt_turn(
                &self,
                _context: GatewayRequestContext,
                _thread_id: String,
                _turn_id: String,
            ) -> Result<InterruptTurnResponse, GatewayError> {
                Err(GatewayError::NotFound(
                    "turn not found: turn-404".to_string(),
                ))
            }

            async fn resolve_server_request(
                &self,
                _context: GatewayRequestContext,
                _request: ResolveServerRequestRequest,
            ) -> Result<ResolveServerRequestResponse, GatewayError> {
                Err(GatewayError::NotFound(
                    "server request not found: req-404".to_string(),
                ))
            }

            fn health(&self) -> GatewayHealthResponse {
                GatewayHealthResponse {
                    status: GatewayHealthStatus::Unavailable,
                    runtime_mode: "remote".to_string(),
                    execution_mode: GatewayExecutionMode::WorkerManaged,
                    v2_compatibility: GatewayV2CompatibilityMode::RemoteMultiWorker,
                    v2_transport: GatewayV2TransportConfig {
                        initialize_timeout_seconds: 30,
                        client_send_timeout_seconds: 10,
                        reconnect_retry_backoff_seconds: 1,
                        max_pending_server_requests: 64,
                    },
                    v2_connections: default_v2_connections(),
                    remote_workers: Some(Vec::new()),
                }
            }

            fn subscribe(&self) -> broadcast::Receiver<GatewayEvent> {
                let (events, _rx) = broadcast::channel(1);
                events.subscribe()
            }

            fn event_visible_to(
                &self,
                _context: &GatewayRequestContext,
                _event: &GatewayEvent,
            ) -> bool {
                true
            }
        }

        let app = router(
            Arc::new(ErrorRuntime),
            GatewayAuth::Disabled,
            GatewayAdmissionController::default(),
        );

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/threads")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"cwd":"/tmp/project"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(create_response.status(), StatusCode::BAD_REQUEST);

        let read_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/threads/thread-404")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(read_response.status(), StatusCode::NOT_FOUND);

        let turn_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/threads/thread-123/turns")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"input":"hello"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(turn_response.status(), StatusCode::BAD_GATEWAY);

        let interrupt_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/threads/thread-123/turns/turn-404/interrupt")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(interrupt_response.status(), StatusCode::NOT_FOUND);

        let resolve_response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/server-requests/respond")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"requestId":"req-404","type":"toolRequestUserInput","answers":{}}"#,
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(resolve_response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn router_emits_request_metrics() {
        let exporter = InMemoryMetricExporter::default();
        let metrics = codex_otel::MetricsClient::new(codex_otel::MetricsConfig::in_memory(
            "test",
            "codex-gateway",
            env!("CARGO_PKG_VERSION"),
            exporter.clone(),
        ))
        .expect("metrics");
        let app = router_with_observability(
            Arc::new(FakeRuntime::default()),
            GatewayAuth::Disabled,
            GatewayAdmissionController::default(),
            GatewayObservability::new(Some(metrics), false),
            Arc::new(GatewayScopeRegistry::default()),
            None,
            GatewayV2Timeouts::default(),
        );

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/threads")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"cwd":"/tmp/project"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);

        let resource_metrics = exporter
            .get_finished_metrics()
            .expect("finished metrics")
            .into_iter()
            .last()
            .expect("latest metrics");
        let metrics = resource_metrics
            .scope_metrics()
            .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

        let mut saw_count = false;
        let mut saw_duration = false;
        for metric in metrics {
            match metric.name() {
                "gateway_http_requests" => {
                    saw_count = true;
                    match metric.data() {
                        AggregatedMetrics::U64(data) => match data {
                            MetricData::Sum(sum) => {
                                let point = sum.data_points().next().expect("count point");
                                assert_eq!(point.value(), 1);
                            }
                            _ => panic!("unexpected request count aggregation"),
                        },
                        _ => panic!("unexpected request count type"),
                    }
                }
                "gateway_http_request_duration" => {
                    saw_duration = true;
                    match metric.data() {
                        AggregatedMetrics::F64(data) => match data {
                            MetricData::Histogram(histogram) => {
                                let point =
                                    histogram.data_points().next().expect("histogram point");
                                assert_eq!(point.count(), 1);
                            }
                            _ => panic!("unexpected duration aggregation"),
                        },
                        _ => panic!("unexpected duration type"),
                    }
                }
                _ => {}
            }
        }

        assert_eq!(saw_count, true);
        assert_eq!(saw_duration, true);
    }

    #[tokio::test]
    async fn router_rate_limits_requests_per_scope() {
        let app = router(
            Arc::new(FakeRuntime::default()),
            GatewayAuth::Disabled,
            GatewayAdmissionController::new(GatewayAdmissionConfig {
                request_rate_limit_per_minute: Some(1),
                turn_start_quota_per_minute: None,
            }),
        );

        let first_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/threads")
                    .header("content-type", "application/json")
                    .header("x-codex-tenant-id", "tenant-a")
                    .body(Body::from(r#"{"cwd":"/tmp/project"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(first_response.status(), StatusCode::OK);

        let limited_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/threads")
                    .header("content-type", "application/json")
                    .header("x-codex-tenant-id", "tenant-a")
                    .body(Body::from(r#"{"cwd":"/tmp/project"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(limited_response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            limited_response
                .headers()
                .get("retry-after")
                .and_then(|value| value.to_str().ok())
                .is_some(),
            true
        );

        let other_scope_response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/threads")
                    .header("content-type", "application/json")
                    .header("x-codex-tenant-id", "tenant-b")
                    .body(Body::from(r#"{"cwd":"/tmp/project"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(other_scope_response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn router_enforces_turn_start_quota() {
        let app = router(
            Arc::new(FakeRuntime::default()),
            GatewayAuth::Disabled,
            GatewayAdmissionController::new(GatewayAdmissionConfig {
                request_rate_limit_per_minute: None,
                turn_start_quota_per_minute: Some(1),
            }),
        );

        let first_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/threads/thread-1/turns")
                    .header("content-type", "application/json")
                    .header("x-codex-tenant-id", "tenant-a")
                    .body(Body::from(r#"{"input":"hello"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(first_response.status(), StatusCode::OK);

        let limited_response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/threads/thread-1/turns")
                    .header("content-type", "application/json")
                    .header("x-codex-tenant-id", "tenant-a")
                    .body(Body::from(r#"{"input":"hello again"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(limited_response.status(), StatusCode::TOO_MANY_REQUESTS);
    }
}
