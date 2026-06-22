mod convert;
pub(crate) mod dto;
pub(crate) mod error;
pub(crate) mod handlers;

use crate::api::AppState;
use axum::{routing::post, Router};
use handlers::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/rpc/Lyre/GetRoom", post(get_room))
        .route("/rpc/Lyre/JoinRoom", post(join_room))
        .route("/rpc/Lyre/LeaveRoom", post(leave_room))
        .route("/rpc/Lyre/GetNoiseProviders", post(get_noise_providers))
        .route("/rpc/Lyre/GetIceServers", post(get_ice_servers))
        .route("/rpc/Lyre/GetMediaTopology", post(get_media_topology))
        .route("/rpc/Lyre/GetMediaRelay", post(get_media_relay))
        .route("/rpc/Lyre/StartMediaRelay", post(start_media_relay))
        .route("/rpc/Lyre/StopMediaRelay", post(stop_media_relay))
        .route("/rpc/Lyre/RegisterMediaTrack", post(register_media_track))
        .route(
            "/rpc/Lyre/RegisterMediaParticipant",
            post(register_media_participant),
        )
        .route(
            "/rpc/Lyre/UpdateMediaRelaySubscriptions",
            post(update_media_relay_subscriptions),
        )
        .route(
            "/rpc/Lyre/AnswerServerMediaOffer",
            post(answer_server_media_offer),
        )
        .route(
            "/rpc/Lyre/AddServerMediaIceCandidate",
            post(add_server_media_ice_candidate),
        )
        .route(
            "/rpc/Lyre/GetServerMediaIceCandidates",
            post(get_server_media_ice_candidates),
        )
        .route(
            "/rpc/Lyre/CloseServerMediaSession",
            post(close_server_media_session),
        )
}
