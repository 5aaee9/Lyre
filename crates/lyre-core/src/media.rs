mod helpers;
mod types;

use crate::{NoiseCancellationConfig, RoomId, UserId};
use dashmap::DashMap;
use helpers::{
    inactive_status, participant_has_audio_track, sorted_participants,
    sorted_remote_audio_source_ids, sorted_subscription_ids, sorted_unique_remote_user_ids,
};
pub use types::{
    MediaRelayError, MediaRelayMode, MediaRelayParticipant, MediaRelayRegistryAggregate,
    MediaRelayRoomStatus, MediaRelayStatus, MediaRelaySubscriptions, MediaRelayTrack,
    MediaRelayTrackLookup, MediaTrackKind, RegisterMediaTrackRequest, StartMediaRelayRequest,
    StopMediaRelayRequest, UpdateMediaRelaySettingsRequest, UpdateMediaRelaySubscriptionsRequest,
};

#[derive(Debug, Clone, Default)]
struct MediaRelayRoomState {
    active: bool,
    noise: NoiseCancellationConfig,
    participants: DashMap<UserId, DashMap<String, MediaTrackKind>>,
    subscriptions: DashMap<UserId, DashMap<UserId, ()>>,
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
        self.rooms.remove(&room_id);
        self.snapshot(room_id)
    }

    pub fn register_participant(
        &self,
        room_id: RoomId,
        user_id: UserId,
    ) -> Result<MediaRelayRoomStatus, MediaRelayError> {
        let Some(room) = self.rooms.get(&room_id) else {
            return Err(MediaRelayError::Inactive { room_id });
        };
        if !room.active {
            return Err(MediaRelayError::Inactive { room_id });
        }
        room.participants.entry(user_id).or_default().clear();
        drop(room);
        Ok(self.snapshot(room_id))
    }

    pub fn register_track(
        &self,
        room_id: RoomId,
        request: RegisterMediaTrackRequest,
    ) -> Result<MediaRelayRoomStatus, MediaRelayError> {
        let Some(room) = self.rooms.get(&room_id) else {
            return Err(MediaRelayError::Inactive { room_id });
        };
        if !room.active {
            return Err(MediaRelayError::Inactive { room_id });
        }
        let tracks = room.participants.entry(request.user_id).or_default();
        tracks.retain(|track_id, kind| track_id == &request.track_id || *kind != request.kind);
        tracks.insert(request.track_id, request.kind);
        drop(tracks);
        drop(room);
        Ok(self.snapshot(room_id))
    }

    pub fn update_settings(
        &self,
        room_id: RoomId,
        request: UpdateMediaRelaySettingsRequest,
    ) -> Result<MediaRelayRoomStatus, MediaRelayError> {
        let Some(mut room) = self.rooms.get_mut(&room_id) else {
            return Err(MediaRelayError::Inactive { room_id });
        };
        if !room.active {
            return Err(MediaRelayError::Inactive { room_id });
        }
        if !room.participants.contains_key(&request.user_id) {
            return Err(MediaRelayError::ParticipantNotFound {
                room_id,
                user_id: request.user_id,
            });
        }
        room.noise = request.noise;
        drop(room);
        Ok(self.snapshot(room_id))
    }

    pub fn remove_participant(
        &self,
        room_id: RoomId,
        user_id: &UserId,
    ) -> Result<MediaRelayRoomStatus, MediaRelayError> {
        let Some(room) = self.rooms.get_mut(&room_id) else {
            return Err(MediaRelayError::Inactive { room_id });
        };
        if !room.active {
            return Err(MediaRelayError::Inactive { room_id });
        }
        room.participants.remove(user_id);
        room.subscriptions.remove(user_id);
        for subscription in &room.subscriptions {
            subscription.value().remove(user_id);
        }
        drop(room);
        Ok(self.snapshot(room_id))
    }

    pub fn update_subscriptions(
        &self,
        room_id: RoomId,
        request: UpdateMediaRelaySubscriptionsRequest,
    ) -> Result<MediaRelaySubscriptions, MediaRelayError> {
        let Some(room) = self.rooms.get_mut(&room_id) else {
            return Err(MediaRelayError::Inactive { room_id });
        };
        if !room.active {
            return Err(MediaRelayError::Inactive { room_id });
        }
        if !room.participants.contains_key(&request.user_id) {
            return Err(MediaRelayError::ParticipantNotFound {
                room_id,
                user_id: request.user_id,
            });
        }
        for source_user_id in &request.source_user_ids {
            if !room
                .participants
                .get(source_user_id)
                .is_some_and(|participant| participant_has_audio_track(participant.value()))
            {
                return Err(MediaRelayError::ParticipantNotFound {
                    room_id,
                    user_id: source_user_id.clone(),
                });
            }
        }

        let source_user_ids =
            sorted_unique_remote_user_ids(request.source_user_ids, &request.user_id);
        let sources = room
            .subscriptions
            .entry(request.user_id.clone())
            .or_default();
        sources.clear();
        for source_user_id in &source_user_ids {
            sources.insert(source_user_id.clone(), ());
        }

        Ok(MediaRelaySubscriptions {
            room_id,
            user_id: request.user_id,
            source_user_ids,
        })
    }

    pub fn subscriptions(
        &self,
        room_id: &RoomId,
        user_id: &UserId,
    ) -> Result<MediaRelaySubscriptions, MediaRelayError> {
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
        if !room.participants.contains_key(user_id) {
            return Err(MediaRelayError::ParticipantNotFound {
                room_id: room_id.clone(),
                user_id: user_id.clone(),
            });
        }

        let source_user_ids = room
            .subscriptions
            .get(user_id)
            .map(|sources| sorted_subscription_ids(sources.value()))
            .unwrap_or_else(|| sorted_remote_audio_source_ids(&room.participants, user_id));
        Ok(MediaRelaySubscriptions {
            room_id: room_id.clone(),
            user_id: user_id.clone(),
            source_user_ids,
        })
    }

    pub fn is_source_subscribed(
        &self,
        room_id: &RoomId,
        recipient_user_id: &UserId,
        source_user_id: &UserId,
    ) -> Result<bool, MediaRelayError> {
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
        if !room.participants.contains_key(recipient_user_id) {
            return Err(MediaRelayError::ParticipantNotFound {
                room_id: room_id.clone(),
                user_id: recipient_user_id.clone(),
            });
        }
        if recipient_user_id == source_user_id {
            return Ok(false);
        }
        if !room.participants.contains_key(source_user_id) {
            return Err(MediaRelayError::ParticipantNotFound {
                room_id: room_id.clone(),
                user_id: source_user_id.clone(),
            });
        }
        if !room
            .participants
            .get(source_user_id)
            .is_some_and(|participant| participant_has_audio_track(participant.value()))
        {
            return Ok(false);
        }

        Ok(room
            .subscriptions
            .get(recipient_user_id)
            .map(|sources| sources.contains_key(source_user_id))
            .unwrap_or(true))
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
            mode: MediaRelayMode::MediaRelay,
            server_side_audio_processing: false,
            server_side_noise_cancelling: false,
            noise: room.noise.clone(),
            participants: sorted_participants(&room.participants),
        }
    }
}
