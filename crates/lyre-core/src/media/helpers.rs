use super::{
    MediaRelayMode, MediaRelayParticipant, MediaRelayRoomStatus, MediaRelayStatus, MediaRelayTrack,
    MediaTrackKind,
};
use crate::{NoiseCancellationConfig, RoomId, UserId};
use dashmap::DashMap;

pub(super) fn sorted_participants(
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

pub(super) fn sorted_remote_audio_source_ids(
    participants: &DashMap<UserId, DashMap<String, MediaTrackKind>>,
    recipient_user_id: &UserId,
) -> Vec<UserId> {
    let mut user_ids = participants
        .iter()
        .filter(|entry| {
            entry.key() != recipient_user_id && participant_has_audio_track(entry.value())
        })
        .map(|entry| entry.key().clone())
        .collect::<Vec<_>>();
    user_ids.sort();
    user_ids
}

pub(super) fn participant_has_audio_track(participant: &DashMap<String, MediaTrackKind>) -> bool {
    participant
        .iter()
        .any(|track| *track.value() == MediaTrackKind::Audio)
}

pub(super) fn sorted_subscription_ids(subscriptions: &DashMap<UserId, ()>) -> Vec<UserId> {
    let mut source_user_ids = subscriptions
        .iter()
        .map(|entry| entry.key().clone())
        .collect::<Vec<_>>();
    source_user_ids.sort();
    source_user_ids
}

pub(super) fn sorted_unique_remote_user_ids(
    mut source_user_ids: Vec<UserId>,
    recipient_user_id: &UserId,
) -> Vec<UserId> {
    source_user_ids.retain(|source_user_id| source_user_id != recipient_user_id);
    source_user_ids.sort();
    source_user_ids.dedup();
    source_user_ids
}

pub(super) fn inactive_status(room_id: RoomId) -> MediaRelayRoomStatus {
    MediaRelayRoomStatus {
        room_id,
        status: MediaRelayStatus::Inactive,
        mode: MediaRelayMode::MediaRelay,
        server_side_audio_processing: false,
        server_side_noise_cancelling: false,
        noise: NoiseCancellationConfig::default(),
        participants: Vec::new(),
    }
}
