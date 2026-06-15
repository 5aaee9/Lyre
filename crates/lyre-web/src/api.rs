use crate::{
    error::ApiError,
    signalling::{route_signal_message, SignalMessage, SignalPayload},
};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::{header, HeaderMap, StatusCode, Uri},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use lyre_core::{
    supported_noise_providers, IceServerConfig, JoinRoomRequest, LeaveRoomRequest, RoomId,
};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tower_http::{
    cors::CorsLayer,
    trace::{DefaultOnResponse, TraceLayer},
};
use tracing::Level;

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
        .merge(crate::api_server_media::router())
        .merge(crate::webrpc::router())
        .route("/api/rooms/{room_id}/ws", get(room_ws))
        .layer(CorsLayer::permissive())
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(make_request_span)
                .on_response(DefaultOnResponse::new()),
        )
        .with_state(state)
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
) -> Result<(), ApiError> {
    let token = bearer_token(headers)?;
    state
        .registry
        .validate_access_token(room_id, user_id, &token)
        .map_err(|_| ApiError::Unauthorized)
}

pub(crate) fn authorize_room_member(
    state: &AppState,
    room_id: &RoomId,
    headers: &HeaderMap,
) -> Result<(), ApiError> {
    let token = bearer_token(headers)?;
    state
        .registry
        .validate_any_access_token(room_id, &token)
        .map_err(|_| ApiError::Unauthorized)
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
    let snapshot = state
        .leave_room_persisted(&room_id, &request.user_id)
        .await?;
    state.peers.user_left(&room_id, &request.user_id);
    Ok(Json(snapshot))
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
    authorize_room_member(&state, &room_id, &headers)?;
    Ok(Json(state.start_media_relay(room_id, request)))
}

async fn stop_media_relay(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<lyre_core::StopMediaRelayRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    authorize_room_user(&state, &room_id, &request.user_id, &headers)?;
    Ok(Json(state.stop_media_relay(room_id, request)))
}

async fn register_media_track(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<lyre_core::RegisterMediaTrackRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    authorize_room_user(&state, &room_id, &request.user_id, &headers)?;
    Ok(Json(state.media_relays.register_track(room_id, request)?))
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
    Ok(ws.on_upgrade(move |socket| handle_socket(state, room_id, user_id, socket)))
}

async fn handle_socket(
    state: AppState,
    room_id: RoomId,
    user_id: lyre_core::UserId,
    socket: WebSocket,
) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (peer_tx, mut peer_rx) = mpsc::unbounded_channel();
    let snapshot = state
        .peers
        .connect(&state.registry, room_id.clone(), user_id.clone(), peer_tx);
    let snapshot_message = SignalMessage::new(
        room_id.clone(),
        user_id.clone(),
        Some(user_id.clone()),
        SignalPayload::RoomSnapshot { room: snapshot },
    );
    let _ = ws_tx
        .send(Message::Text(
            serde_json::to_string(&snapshot_message).unwrap().into(),
        ))
        .await;

    loop {
        tokio::select! {
            Some(message) = peer_rx.recv() => {
                match serde_json::to_string(&message) {
                    Ok(json) => {
                        if ws_tx.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        tracing::debug!(error = %error, "failed to serialize signal message");
                    }
                }
            }
            Some(Ok(message)) = ws_rx.next() => {
                if let Message::Text(text) = message {
                    match serde_json::from_str::<SignalMessage>(&text) {
                        Ok(signal) => match route_signal_message(&room_id, &user_id, &signal) {
                            Ok(_) => {
                                state.peers.forward(signal);
                            }
                            Err(error) => {
                                let _ = ws_tx.send(Message::Text(serde_json::to_string(&*error).unwrap().into())).await;
                            }
                        },
                        Err(_) => {
                            let error = SignalMessage::error(room_id.clone(), user_id.clone(), "invalid signal message");
                            let _ = ws_tx.send(Message::Text(serde_json::to_string(&error).unwrap().into())).await;
                        }
                    }
                }
            }
            else => break,
        }
    }

    state.peers.disconnect(&room_id, &user_id);
}
