use crate::{api_websocket::handle_socket, error::ApiError};
use axum::{
    extract::{ws::WebSocketUpgrade, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode, Uri},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use lyre_core::{
    supported_noise_providers, IceServerConfig, JoinRoomRequest, LeaveRoomRequest, RoomId, UserId,
};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    trace::{DefaultOnResponse, TraceLayer},
};
use tracing::Level;

#[cfg(test)]
pub(crate) use crate::api_websocket::{handle_signal_message, handle_socket_io};
pub use crate::app_state::AppState;

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Debug, Deserialize)]
struct WsQuery {
    user_id: String,
    access_token: Option<String>,
}

pub fn router(state: AppState) -> Router {
    router_with_profile(state, crate::profile::enabled_from_env())
}

pub fn router_with_profile(state: AppState, profile_enabled: bool) -> Router {
    router_from_base(profiled_base_router(profile_enabled), state)
}

pub fn router_with_cors(state: AppState, allowed_origins: Vec<String>) -> anyhow::Result<Router> {
    router_with_cors_and_profile(state, allowed_origins, crate::profile::enabled_from_env())
}

pub fn router_with_cors_and_profile(
    state: AppState,
    allowed_origins: Vec<String>,
    profile_enabled: bool,
) -> anyhow::Result<Router> {
    let allowed_origins = allowed_origins
        .into_iter()
        .map(|origin| HeaderValue::from_str(origin.trim()))
        .collect::<Result<Vec<_>, _>>()?;
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(allowed_origins))
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    Ok(router_from_base(
        profiled_base_router(profile_enabled).layer(cors),
        state,
    ))
}

fn router_from_base(router: Router<AppState>, state: AppState) -> Router {
    router
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(make_request_span)
                .on_response(DefaultOnResponse::new()),
        )
        .with_state(state)
}

fn profiled_base_router(profile_enabled: bool) -> Router<AppState> {
    let router = base_router();
    if profile_enabled {
        router.merge(crate::profile::router())
    } else {
        router
    }
}

fn base_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/metrics", get(crate::metrics::metrics))
        .route("/api/noise/providers", get(noise_providers))
        .route("/api/webrtc/ice-servers", get(ice_servers))
        .route("/api/webrtc/topology", get(media_topology))
        .route("/api/rooms/{room_id}", get(room_snapshot))
        .route("/api/rooms/{room_id}/join", post(join_room))
        .route("/api/rooms/{room_id}/leave", post(leave_room))
        .route("/api/rooms/{room_id}/media-relay", get(media_relay_status))
        .route(
            "/api/rooms/{room_id}/media-relay/start",
            post(start_media_relay),
        )
        .route(
            "/api/rooms/{room_id}/media-relay/stop",
            post(stop_media_relay),
        )
        .route(
            "/api/rooms/{room_id}/media-relay/tracks",
            post(register_media_track),
        )
        .route(
            "/api/rooms/{room_id}/media-relay/participants",
            post(register_media_participant),
        )
        .route(
            "/api/rooms/{room_id}/media-relay/subscriptions",
            post(update_media_relay_subscriptions),
        )
        .route(
            "/api/rooms/{room_id}/media-relay/settings",
            post(update_media_relay_settings),
        )
        .merge(crate::api_server_media::router())
        .merge(crate::webrpc::router())
        .route("/api/rooms/{room_id}/ws", get(room_ws))
}

fn make_request_span<B>(request: &axum::http::Request<B>) -> tracing::Span {
    tracing::span!(
        Level::INFO,
        "request",
        method = %request.method(),
        path = redacted_trace_path(request.uri()),
    )
}

pub(crate) fn redacted_trace_path(uri: &Uri) -> &str {
    uri.path()
}

fn bearer_token(headers: &HeaderMap) -> Result<lyre_core::RoomAccessToken, ApiError> {
    let Some(value) = headers.get(header::AUTHORIZATION) else {
        return Err(ApiError::Unauthorized);
    };
    let value = value.to_str().map_err(|_| ApiError::Unauthorized)?;
    let Some(token) = value.strip_prefix("Bearer ") else {
        return Err(ApiError::Unauthorized);
    };
    if token.is_empty() {
        return Err(ApiError::Unauthorized);
    }
    Ok(lyre_core::RoomAccessToken::from_external(token))
}

pub(crate) fn authorize_room_user(
    state: &AppState,
    room_id: &RoomId,
    user_id: &lyre_core::UserId,
    headers: &HeaderMap,
) -> Result<lyre_core::RoomAccessToken, ApiError> {
    let token = bearer_token(headers)?;
    state
        .registry
        .validate_access_token(room_id, user_id, &token)
        .map_err(|_| ApiError::Unauthorized)?;
    Ok(token)
}

pub(crate) fn authorize_room_member(
    state: &AppState,
    room_id: &RoomId,
    headers: &HeaderMap,
) -> Result<lyre_core::RoomAccessToken, ApiError> {
    let token = bearer_token(headers)?;
    state
        .registry
        .validate_any_access_token(room_id, &token)
        .map_err(|_| ApiError::Unauthorized)?;
    Ok(token)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn noise_providers() -> Json<Vec<lyre_core::NoiseCancellationConfig>> {
    Json(supported_noise_providers())
}

async fn ice_servers(
    State(state): State<AppState>,
) -> Result<Json<Vec<IceServerConfig>>, ApiError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after unix epoch")
        .as_secs();
    let servers = lyre_core::ice_servers_with_turn_rest_credentials(
        &state.ice_servers,
        state.turn_rest_credentials.as_ref(),
        now,
    )?;
    Ok(Json(servers))
}

async fn media_topology() -> Json<lyre_core::MediaTopology> {
    Json(lyre_core::current_media_topology())
}

async fn room_snapshot(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    Ok(Json(state.registry.snapshot(room_id)))
}

async fn join_room(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(request): Json<JoinRoomRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    let response = state.join_room_persisted(room_id.clone(), request).await?;
    state.peers.user_joined(&room_id, response.user.clone());
    Ok((StatusCode::CREATED, Json(response)))
}

async fn leave_room(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<LeaveRoomRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    authorize_room_user(&state, &room_id, &request.user_id, &headers)?;
    let response = state
        .leave_room_persisted(&room_id, &request.user_id)
        .await?;
    if response.removed {
        state.peers.remove_peer(&room_id, &request.user_id);
        state.peers.user_left(&room_id, &request.user_id);
    }
    Ok(Json(response.room))
}

async fn media_relay_status(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    Ok(Json(state.media_relays.status(room_id)))
}

async fn start_media_relay(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<lyre_core::StartMediaRelayRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    let access_token = authorize_room_member(&state, &room_id, &headers)?;
    Ok(Json(
        state
            .start_authorized_media_relay(room_id, request, &access_token)
            .await?,
    ))
}

async fn stop_media_relay(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<lyre_core::StopMediaRelayRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    let access_token = authorize_room_user(&state, &room_id, &request.user_id, &headers)?;
    Ok(Json(
        state
            .stop_authorized_media_relay(room_id, request, &access_token)
            .await?,
    ))
}

async fn register_media_track(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<lyre_core::RegisterMediaTrackRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    let access_token = authorize_room_user(&state, &room_id, &request.user_id, &headers)?;
    Ok(Json(
        state
            .register_authorized_media_track(room_id, request, &access_token)
            .await?,
    ))
}

#[derive(Deserialize)]
struct RegisterMediaParticipantRequest {
    user_id: UserId,
}

async fn register_media_participant(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<RegisterMediaParticipantRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    let access_token = authorize_room_user(&state, &room_id, &request.user_id, &headers)?;
    Ok(Json(
        state
            .register_authorized_media_participant(room_id, request.user_id, &access_token)
            .await?,
    ))
}

async fn update_media_relay_subscriptions(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<lyre_core::media::UpdateMediaRelaySubscriptionsRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    let access_token = authorize_room_user(&state, &room_id, &request.user_id, &headers)?;
    Ok(Json(
        state
            .update_authorized_media_relay_subscriptions(room_id, request, &access_token)
            .await?,
    ))
}

async fn update_media_relay_settings(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<lyre_core::UpdateMediaRelaySettingsRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    let access_token = authorize_room_user(&state, &room_id, &request.user_id, &headers)?;
    Ok(Json(
        state
            .update_authorized_media_relay_settings(room_id, request, &access_token)
            .await?,
    ))
}

async fn room_ws(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    let user_id = lyre_core::UserId::from_external(query.user_id);
    let Some(access_token) = query.access_token else {
        return Err(ApiError::Unauthorized);
    };
    let token = lyre_core::RoomAccessToken::from_external(access_token);
    state
        .registry
        .validate_access_token(&room_id, &user_id, &token)
        .map_err(|_| ApiError::Unauthorized)?;
    Ok(ws.on_upgrade(move |socket| handle_socket(state, room_id, user_id, token, socket)))
}
