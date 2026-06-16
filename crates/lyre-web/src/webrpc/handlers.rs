use super::{dto, error::WebrpcError};
use crate::api::{authorize_room_member, authorize_room_user, AppState};
use axum::{body::Bytes, extract::State, http::HeaderMap, Json};
use lyre_core::{RoomId, UserId};
use lyre_webrtc::{ServerMediaIceCandidate, ServerMediaOffer, ServerMediaSessionKey};
use serde::de::DeserializeOwned;
use std::time::{SystemTime, UNIX_EPOCH};

fn parse_json<T: DeserializeOwned>(bytes: Bytes) -> Result<T, WebrpcError> {
    serde_json::from_slice(&bytes).map_err(|_| WebrpcError::bad_request())
}

pub(crate) async fn get_room(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Json<dto::GetRoomResponse>, WebrpcError> {
    let request: dto::GetRoomRequest = parse_json(body)?;
    let room_id = RoomId::parse_boundary(request.room_id)?;
    Ok(Json(dto::GetRoomResponse {
        room: state.registry.snapshot(room_id).into(),
    }))
}

pub(crate) async fn join_room(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Json<dto::JoinRoomResponse>, WebrpcError> {
    let request: dto::JoinRoomRequest = parse_json(body)?;
    let room_id = RoomId::parse_boundary(request.room_id)?;
    let response = state
        .join_room_persisted(
            room_id.clone(),
            lyre_core::JoinRoomRequest {
                nickname: request.nickname,
                noise: request.noise.map(Into::into),
            },
        )
        .await?;
    state.peers.user_joined(&room_id, response.user.clone());
    Ok(Json(dto::JoinRoomResponse {
        user: response.user.into(),
        room: response.room.into(),
        access_token: response.access_token.as_str().to_owned(),
    }))
}

pub(crate) async fn leave_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<dto::LeaveRoomResponse>, WebrpcError> {
    let request: dto::LeaveRoomRequest = parse_json(body)?;
    let room_id = RoomId::parse_boundary(request.room_id)?;
    let user_id = UserId::from_external(request.user_id);
    authorize_room_user(&state, &room_id, &user_id, &headers)?;
    let response = state.leave_room_persisted(&room_id, &user_id).await?;
    if response.removed {
        state.peers.user_left(&room_id, &user_id);
    }
    Ok(Json(dto::LeaveRoomResponse {
        room: response.room.into(),
    }))
}

pub(crate) async fn get_noise_providers() -> Json<dto::GetNoiseProvidersResponse> {
    Json(dto::GetNoiseProvidersResponse {
        providers: lyre_core::supported_noise_providers()
            .into_iter()
            .map(Into::into)
            .collect(),
    })
}

pub(crate) async fn get_ice_servers(
    State(state): State<AppState>,
) -> Result<Json<dto::GetIceServersResponse>, WebrpcError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after unix epoch")
        .as_secs();
    let ice_servers = lyre_core::ice_servers_with_turn_rest_credentials(
        &state.ice_servers,
        state.turn_rest_credentials.as_ref(),
        now,
    )?
    .into_iter()
    .map(Into::into)
    .collect();
    Ok(Json(dto::GetIceServersResponse { ice_servers }))
}

pub(crate) async fn get_media_topology() -> Json<dto::GetMediaTopologyResponse> {
    Json(dto::GetMediaTopologyResponse {
        topology: lyre_core::current_media_topology().into(),
    })
}

pub(crate) async fn get_media_relay(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Json<dto::GetMediaRelayResponse>, WebrpcError> {
    let request: dto::GetMediaRelayRequest = parse_json(body)?;
    let room_id = RoomId::parse_boundary(request.room_id)?;
    Ok(Json(dto::GetMediaRelayResponse {
        media_relay: state.media_relays.status(room_id).into(),
    }))
}

pub(crate) async fn start_media_relay(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<dto::StartMediaRelayResponse>, WebrpcError> {
    let request: dto::StartMediaRelayRequest = parse_json(body)?;
    let room_id = RoomId::parse_boundary(request.room_id)?;
    authorize_room_member(&state, &room_id, &headers)?;
    let media_relay = state.start_media_relay(
        room_id,
        lyre_core::StartMediaRelayRequest {
            noise: request.noise.map(Into::into),
        },
    );
    Ok(Json(dto::StartMediaRelayResponse {
        media_relay: media_relay.into(),
    }))
}

pub(crate) async fn stop_media_relay(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<dto::StopMediaRelayResponse>, WebrpcError> {
    let request: dto::StopMediaRelayRequest = parse_json(body)?;
    let room_id = RoomId::parse_boundary(request.room_id)?;
    let user_id = UserId::from_external(request.user_id);
    authorize_room_user(&state, &room_id, &user_id, &headers)?;
    let media_relay = state.stop_media_relay(room_id, lyre_core::StopMediaRelayRequest { user_id });
    Ok(Json(dto::StopMediaRelayResponse {
        media_relay: media_relay.into(),
    }))
}

pub(crate) async fn register_media_track(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<dto::RegisterMediaTrackResponse>, WebrpcError> {
    let request: dto::RegisterMediaTrackRequest = parse_json(body)?;
    let room_id = RoomId::parse_boundary(request.room_id)?;
    let user_id = UserId::from_external(request.user_id);
    authorize_room_user(&state, &room_id, &user_id, &headers)?;
    let media_relay = state.media_relays.register_track(
        room_id,
        lyre_core::RegisterMediaTrackRequest {
            user_id,
            track_id: request.track_id,
            kind: request.kind.into(),
        },
    )?;
    Ok(Json(dto::RegisterMediaTrackResponse {
        media_relay: media_relay.into(),
    }))
}

pub(crate) async fn answer_server_media_offer(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<dto::AnswerServerMediaOfferResponse>, WebrpcError> {
    let request: dto::AnswerServerMediaOfferRequest = parse_json(body)?;
    let room_id = RoomId::parse_boundary(request.room_id)?;
    let user_id = UserId::from_external(request.user_id);
    authorize_room_user(&state, &room_id, &user_id, &headers)?;
    let answer = state
        .answer_server_media_offer(ServerMediaOffer {
            room_id,
            user_id,
            audio_track_id: request.audio_track_id,
            sdp: request.sdp,
        })
        .await?;
    Ok(Json(dto::AnswerServerMediaOfferResponse {
        answer: answer.into(),
    }))
}

pub(crate) async fn add_server_media_ice_candidate(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<dto::AddServerMediaIceCandidateResponse>, WebrpcError> {
    let request: dto::AddServerMediaIceCandidateRequest = parse_json(body)?;
    let room_id = RoomId::parse_boundary(request.room_id)?;
    let user_id = UserId::from_external(request.user_id);
    authorize_room_user(&state, &room_id, &user_id, &headers)?;
    let candidate = ServerMediaIceCandidate {
        room_id,
        user_id,
        candidate: request.candidate,
        sdp_mid: request.sdp_mid,
        sdp_mline_index: request.sdp_mline_index,
        username_fragment: request.username_fragment,
    };
    state
        .add_server_media_ice_candidate(candidate.clone())
        .await?;
    Ok(Json(dto::AddServerMediaIceCandidateResponse {
        accepted: candidate.into(),
    }))
}

pub(crate) async fn get_server_media_ice_candidates(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<dto::GetServerMediaIceCandidatesResponse>, WebrpcError> {
    let request: dto::GetServerMediaIceCandidatesRequest = parse_json(body)?;
    let room_id = RoomId::parse_boundary(request.room_id)?;
    let user_id = UserId::from_external(request.user_id);
    authorize_room_user(&state, &room_id, &user_id, &headers)?;
    let candidates = state
        .server_media_ice_candidates(&ServerMediaSessionKey { room_id, user_id })
        .into_iter()
        .map(Into::into)
        .collect();
    Ok(Json(dto::GetServerMediaIceCandidatesResponse {
        candidates,
    }))
}

pub(crate) async fn close_server_media_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<dto::CloseServerMediaSessionResponse>, WebrpcError> {
    let request: dto::CloseServerMediaSessionRequest = parse_json(body)?;
    let room_id = RoomId::parse_boundary(request.room_id)?;
    let user_id = UserId::from_external(request.user_id);
    authorize_room_user(&state, &room_id, &user_id, &headers)?;
    let closed = state.close_server_media_session_for_user(room_id, user_id)?;
    Ok(Json(dto::CloseServerMediaSessionResponse {
        closed: closed.into(),
    }))
}
