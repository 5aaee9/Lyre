use crate::{NoiseCancellationConfig, RoomId, UserId};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaRelayMode {
    P2pMesh,
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

#[derive(Debug, Clone, Default)]
struct MediaRelayRoomState {
    active: bool,
    noise: NoiseCancellationConfig,
    participants: DashMap<UserId, DashMap<String, MediaTrackKind>>,
}

#[derive(Debug, Default)]
pub struct MediaRelayRegistry {
    rooms: DashMap<RoomId, MediaRelayRoomState>,
}

impl MediaRelayRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn status(&self, room_id: RoomId) -> MediaRelayRoomStatus {
        self.rooms.entry(room_id.clone()).or_default();
        self.snapshot(room_id)
    }

    pub fn contains_room(&self, room_id: &RoomId) -> bool {
        self.rooms.contains_key(room_id)
    }

    pub fn start(&self, room_id: RoomId, request: StartMediaRelayRequest) -> MediaRelayRoomStatus {
        let mut room = self.rooms.entry(room_id.clone()).or_default();
        room.active = true;
        room.noise = request.noise.unwrap_or_default();
        drop(room);
        self.snapshot(room_id)
    }

    pub fn stop(&self, room_id: RoomId, _request: StopMediaRelayRequest) -> MediaRelayRoomStatus {
        let mut room = self.rooms.entry(room_id.clone()).or_default();
        room.active = false;
        room.noise = NoiseCancellationConfig::default();
        room.participants.clear();
        drop(room);
        self.snapshot(room_id)
    }

    pub fn register_track(
        &self,
        room_id: RoomId,
        request: RegisterMediaTrackRequest,
    ) -> Result<MediaRelayRoomStatus, MediaRelayError> {
        let room = self.rooms.entry(room_id.clone()).or_default();
        if !room.active {
            return Err(MediaRelayError::Inactive { room_id });
        }
        room.participants
            .entry(request.user_id)
            .or_default()
            .insert(request.track_id, request.kind);
        drop(room);
        Ok(self.snapshot(room_id))
    }

    pub fn remove_participant(
        &self,
        room_id: RoomId,
        user_id: &UserId,
    ) -> Result<MediaRelayRoomStatus, MediaRelayError> {
        let Some(room) = self.rooms.get(&room_id) else {
            return Err(MediaRelayError::Inactive { room_id });
        };
        if !room.active {
            return Err(MediaRelayError::Inactive { room_id });
        }
        room.participants.remove(user_id);
        drop(room);
        Ok(self.snapshot(room_id))
    }

    pub fn require_track(
        &self,
        room_id: &RoomId,
        user_id: &UserId,
        track_id: &str,
    ) -> Result<MediaRelayTrackLookup, MediaRelayError> {
        let Some(room) = self.rooms.get(room_id) else {
            return Err(MediaRelayError::Inactive {
                room_id: room_id.clone(),
            });
        };
        if !room.active {
            return Err(MediaRelayError::Inactive {
                room_id: room_id.clone(),
            });
        }
        let Some(participant) = room.participants.get(user_id) else {
            return Err(MediaRelayError::ParticipantNotFound {
                room_id: room_id.clone(),
                user_id: user_id.clone(),
            });
        };
        let Some(kind) = participant.get(track_id).map(|entry| *entry.value()) else {
            return Err(MediaRelayError::TrackNotFound {
                room_id: room_id.clone(),
                user_id: user_id.clone(),
                track_id: track_id.to_owned(),
            });
        };
        Ok(MediaRelayTrackLookup {
            room_id: room_id.clone(),
            user_id: user_id.clone(),
            track_id: track_id.to_owned(),
            kind,
            noise: room.noise.clone(),
        })
    }

    pub fn active_participants(
        &self,
        room_id: &RoomId,
    ) -> Result<Vec<MediaRelayParticipant>, MediaRelayError> {
        let Some(room) = self.rooms.get(room_id) else {
            return Err(MediaRelayError::Inactive {
                room_id: room_id.clone(),
            });
        };
        if !room.active {
            return Err(MediaRelayError::Inactive {
                room_id: room_id.clone(),
            });
        }
        Ok(sorted_participants(&room.participants))
    }

    pub fn aggregate(&self) -> MediaRelayRegistryAggregate {
        self.rooms.iter().fold(
            MediaRelayRegistryAggregate {
                active_rooms: 0,
                participants: 0,
            },
            |mut aggregate, entry| {
                if entry.value().active {
                    aggregate.active_rooms += 1;
                    aggregate.participants += entry.value().participants.len();
                }
                aggregate
            },
        )
    }

    fn snapshot(&self, room_id: RoomId) -> MediaRelayRoomStatus {
        let Some(room) = self.rooms.get(&room_id) else {
            return inactive_status(room_id);
        };
        let active = room.active;
        MediaRelayRoomStatus {
            room_id,
            status: if active {
                MediaRelayStatus::Active
            } else {
                MediaRelayStatus::Inactive
            },
            mode: if active {
                MediaRelayMode::MediaRelay
            } else {
                MediaRelayMode::P2pMesh
            },
            server_side_audio_processing: false,
            server_side_noise_cancelling: false,
            noise: room.noise.clone(),
            participants: sorted_participants(&room.participants),
        }
    }
}

fn sorted_participants(
    participants: &DashMap<UserId, DashMap<String, MediaTrackKind>>,
) -> Vec<MediaRelayParticipant> {
    let mut participants = participants
        .iter()
        .map(|entry| {
            let mut tracks = entry
                .value()
                .iter()
                .map(|track| MediaRelayTrack {
                    track_id: track.key().clone(),
                    kind: *track.value(),
                })
                .collect::<Vec<_>>();
            tracks.sort_by(|left, right| left.track_id.cmp(&right.track_id));
            MediaRelayParticipant {
                user_id: entry.key().clone(),
                tracks,
            }
        })
        .collect::<Vec<_>>();
    participants.sort_by(|left, right| left.user_id.cmp(&right.user_id));
    participants
}

fn inactive_status(room_id: RoomId) -> MediaRelayRoomStatus {
    MediaRelayRoomStatus {
        room_id,
        status: MediaRelayStatus::Inactive,
        mode: MediaRelayMode::P2pMesh,
        server_side_audio_processing: false,
        server_side_noise_cancelling: false,
        noise: NoiseCancellationConfig::default(),
        participants: Vec::new(),
    }
}
