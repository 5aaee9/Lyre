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

    fn snapshot(&self, room_id: RoomId) -> MediaRelayRoomStatus {
        let Some(room) = self.rooms.get(&room_id) else {
            return inactive_status(room_id);
        };
        let active = room.active;
        let mut participants = room
            .participants
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
            participants,
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NoiseProvider;

    #[test]
    fn default_status_is_inactive() {
        let registry = MediaRelayRegistry::new();
        let status = registry.status(RoomId::default_room());

        assert_eq!(status.status, MediaRelayStatus::Inactive);
        assert_eq!(status.mode, MediaRelayMode::P2pMesh);
        assert!(!status.server_side_audio_processing);
        assert!(!status.server_side_noise_cancelling);
        assert!(status.participants.is_empty());
    }

    #[test]
    fn start_records_default_and_custom_noise() {
        let registry = MediaRelayRegistry::new();
        let default_started =
            registry.start(RoomId::default_room(), StartMediaRelayRequest::default());
        assert_eq!(default_started.status, MediaRelayStatus::Active);
        assert_eq!(default_started.noise.provider, NoiseProvider::Off);

        let custom = NoiseCancellationConfig {
            provider: NoiseProvider::Rnnoise,
            intensity: 0.8,
            voice_activity_threshold: 0.2,
        };
        let status = registry.start(
            RoomId::default_room(),
            StartMediaRelayRequest {
                noise: Some(custom.clone()),
            },
        );
        assert_eq!(status.noise, custom);
    }

    #[test]
    fn registering_track_requires_active_relay() {
        let registry = MediaRelayRegistry::new();
        let room_id = RoomId::default_room();

        assert_eq!(
            registry.register_track(
                room_id.clone(),
                RegisterMediaTrackRequest {
                    user_id: UserId::from_external("user_01"),
                    track_id: "audio-main".to_owned(),
                    kind: MediaTrackKind::Audio,
                },
            ),
            Err(MediaRelayError::Inactive { room_id })
        );
    }

    #[test]
    fn active_relay_tracks_are_stable_and_replace_same_track() {
        let registry = MediaRelayRegistry::new();
        let room_id = RoomId::default_room();
        registry.start(room_id.clone(), StartMediaRelayRequest::default());

        registry
            .register_track(
                room_id.clone(),
                RegisterMediaTrackRequest {
                    user_id: UserId::from_external("user_b"),
                    track_id: "video-main".to_owned(),
                    kind: MediaTrackKind::Video,
                },
            )
            .unwrap();
        registry
            .register_track(
                room_id.clone(),
                RegisterMediaTrackRequest {
                    user_id: UserId::from_external("user_a"),
                    track_id: "audio-main".to_owned(),
                    kind: MediaTrackKind::Audio,
                },
            )
            .unwrap();
        let status = registry
            .register_track(
                room_id,
                RegisterMediaTrackRequest {
                    user_id: UserId::from_external("user_a"),
                    track_id: "audio-main".to_owned(),
                    kind: MediaTrackKind::Video,
                },
            )
            .unwrap();

        assert_eq!(status.participants[0].user_id.as_str(), "user_a");
        assert_eq!(status.participants[1].user_id.as_str(), "user_b");
        assert_eq!(status.participants[0].tracks[0].track_id, "audio-main");
        assert_eq!(status.participants[0].tracks[0].kind, MediaTrackKind::Video);
    }

    #[test]
    fn stop_clears_participants() {
        let registry = MediaRelayRegistry::new();
        let room_id = RoomId::default_room();
        registry.start(room_id.clone(), StartMediaRelayRequest::default());
        registry
            .register_track(
                room_id.clone(),
                RegisterMediaTrackRequest {
                    user_id: UserId::from_external("user_01"),
                    track_id: "audio-main".to_owned(),
                    kind: MediaTrackKind::Audio,
                },
            )
            .unwrap();

        let status = registry.stop(
            room_id,
            StopMediaRelayRequest {
                user_id: UserId::from_external("user_01"),
            },
        );

        assert_eq!(status.status, MediaRelayStatus::Inactive);
        assert!(status.participants.is_empty());
    }
}
