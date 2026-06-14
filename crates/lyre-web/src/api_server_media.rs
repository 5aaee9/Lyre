use crate::{api::AppState, error::ApiError};
use axum::{
    extract::{Path, Query, State},
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
}

async fn answer_server_media_offer(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(request): Json<ServerMediaOfferRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    let answer = state
        .answer_server_media_offer(ServerMediaOffer {
            room_id,
            user_id: request.user_id,
            audio_track_id: request.audio_track_id,
            sdp: request.sdp,
        })
        .await?;
    Ok(Json(answer))
}

async fn add_server_media_ice_candidate(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(request): Json<ServerMediaCandidateRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    let candidate = ServerMediaIceCandidate {
        room_id,
        user_id: request.user_id,
        candidate: request.candidate,
        sdp_mid: request.sdp_mid,
        sdp_mline_index: request.sdp_mline_index,
        username_fragment: request.username_fragment,
    };
    state
        .add_server_media_ice_candidate(candidate.clone())
        .await?;
    Ok(Json(candidate))
}

async fn server_media_ice_candidates(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Query(query): Query<ServerMediaCandidatesQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    Ok(Json(state.server_media_ice_candidates(
        &ServerMediaSessionKey {
            room_id,
            user_id: query.user_id,
        },
    )))
}
