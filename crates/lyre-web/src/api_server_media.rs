use crate::{
    api::{authorize_room_user, AppState},
    error::ApiError,
    server_media_ice_diagnostics::{summarize_candidates, ServerMediaIceCandidateSummary},
};
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use lyre_core::{RoomId, UserId};
use lyre_webrtc::{ServerMediaIceCandidate, ServerMediaOffer, ServerMediaSessionKey};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ServerMediaOfferRequest {
    user_id: UserId,
    audio_track_id: String,
    sdp: String,
}

#[derive(Debug, Deserialize)]
struct ServerMediaCandidateRequest {
    user_id: UserId,
    candidate: String,
    sdp_mid: Option<String>,
    sdp_mline_index: Option<u16>,
    username_fragment: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CloseServerMediaSessionRequest {
    user_id: UserId,
}

#[derive(Debug, Deserialize)]
struct ServerMediaCandidatesQuery {
    user_id: UserId,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/rooms/{room_id}/server-media/offer",
            post(answer_server_media_offer),
        )
        .route(
            "/api/rooms/{room_id}/server-media/candidates",
            post(add_server_media_ice_candidate).get(server_media_ice_candidates),
        )
        .route(
            "/api/rooms/{room_id}/server-media/close",
            post(close_server_media_session),
        )
}

async fn answer_server_media_offer(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<ServerMediaOfferRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    authorize_room_user(&state, &room_id, &request.user_id, &headers)?;
    let answer = state
        .answer_server_media_offer_with_subscriptions(ServerMediaOffer {
            room_id: room_id.clone(),
            user_id: request.user_id,
            audio_track_id: request.audio_track_id,
            sdp: request.sdp,
        })
        .await?;
    tracing::info!(
        room_id = %room_id,
        user_id = %answer.user_id,
        audio_track_id = %answer.audio_track_id,
        "server media offer answered"
    );
    Ok(Json(answer))
}

async fn add_server_media_ice_candidate(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<ServerMediaCandidateRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    authorize_room_user(&state, &room_id, &request.user_id, &headers)?;
    let candidate = ServerMediaIceCandidate {
        room_id,
        user_id: request.user_id,
        candidate: request.candidate,
        sdp_mid: request.sdp_mid,
        sdp_mline_index: request.sdp_mline_index,
        username_fragment: request.username_fragment,
    };
    let summary = ServerMediaIceCandidateSummary::from_candidate(&candidate);
    state
        .add_server_media_ice_candidate(candidate.clone())
        .await?;
    tracing::info!(
        room_id = %candidate.room_id,
        user_id = %candidate.user_id,
        candidate = ?summary,
        "server media remote ICE candidate accepted"
    );
    Ok(Json(candidate))
}

async fn server_media_ice_candidates(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<ServerMediaCandidatesQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    authorize_room_user(&state, &room_id, &query.user_id, &headers)?;
    let key = ServerMediaSessionKey {
        room_id,
        user_id: query.user_id,
    };
    let candidates = state.server_media_ice_candidates(&key);
    let candidate_summaries = summarize_candidates(&candidates);
    tracing::info!(
        room_id = %key.room_id,
        user_id = %key.user_id,
        candidate_count = candidates.len(),
        candidates = ?candidate_summaries,
        "server media local ICE candidates returned"
    );
    Ok(Json(candidates))
}

async fn close_server_media_session(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<CloseServerMediaSessionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    authorize_room_user(&state, &room_id, &request.user_id, &headers)?;
    Ok(Json(state.close_server_media_session_for_user(
        room_id,
        request.user_id,
    )?))
}
