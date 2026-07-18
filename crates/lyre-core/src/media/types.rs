use crate::{NoiseCancellationConfig, RoomId, UserId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaRelayMode {
    MediaRelay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaRelayStatus {
    Inactive,
    Active,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaTrackKind {
    Audio,
    Video,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaRelayTrack {
    pub track_id: String,
    pub kind: MediaTrackKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaRelayParticipant {
    pub user_id: UserId,
    pub tracks: Vec<MediaRelayTrack>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaRelayRoomStatus {
    pub room_id: RoomId,
    pub status: MediaRelayStatus,
    pub mode: MediaRelayMode,
    pub server_side_audio_processing: bool,
    pub server_side_noise_cancelling: bool,
    pub noise: NoiseCancellationConfig,
    pub participants: Vec<MediaRelayParticipant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaRelayRegistryAggregate {
    pub active_rooms: usize,
    pub participants: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StartMediaRelayRequest {
    pub noise: Option<NoiseCancellationConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopMediaRelayRequest {
    pub user_id: UserId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterMediaTrackRequest {
    pub user_id: UserId,
    pub track_id: String,
    pub kind: MediaTrackKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateMediaRelaySettingsRequest {
    pub user_id: UserId,
    pub noise: NoiseCancellationConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateMediaRelaySubscriptionsRequest {
    pub user_id: UserId,
    pub source_user_ids: Vec<UserId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaRelaySubscriptions {
    pub room_id: RoomId,
    pub user_id: UserId,
    pub source_user_ids: Vec<UserId>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MediaRelayError {
    #[error("media relay is not active for room `{room_id}`")]
    Inactive { room_id: RoomId },
    #[error("media relay participant `{user_id}` is not registered in room `{room_id}`")]
    ParticipantNotFound { room_id: RoomId, user_id: UserId },
    #[error("media relay track `{track_id}` is not registered for participant `{user_id}` in room `{room_id}`")]
    TrackNotFound {
        room_id: RoomId,
        user_id: UserId,
        track_id: String,
    },
    #[error("media relay track `{track_id}` for participant `{user_id}` in room `{room_id}` is `{kind:?}`, not audio")]
    UnsupportedTrackKind {
        room_id: RoomId,
        user_id: UserId,
        track_id: String,
        kind: MediaTrackKind,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaRelayTrackLookup {
    pub room_id: RoomId,
    pub user_id: UserId,
    pub track_id: String,
    pub kind: MediaTrackKind,
    pub noise: NoiseCancellationConfig,
}
