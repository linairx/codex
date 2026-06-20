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
#[path = "http_tests.rs"]
mod tests;
