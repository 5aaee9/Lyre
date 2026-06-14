use lyre_core::{MediaRelayError, MediaRelayRegistry, MediaTrackKind, ProcessedAudioFrame, UserId};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub struct ProcessedAudioEgressFrame {
    pub recipient_id: UserId,
    pub frame: ProcessedAudioFrame,
}

#[derive(Debug, Clone)]
pub struct ProcessedAudioEgressFanout {
    relays: Arc<MediaRelayRegistry>,
}

impl ProcessedAudioEgressFanout {
    pub fn new(relays: Arc<MediaRelayRegistry>) -> Self {
        Self { relays }
    }

    pub fn fanout(
        &self,
        frame: &ProcessedAudioFrame,
    ) -> Result<Vec<ProcessedAudioEgressFrame>, MediaRelayError> {
        let lookup = self
            .relays
            .require_track(&frame.room_id, &frame.user_id, &frame.track_id)?;
        if lookup.kind != MediaTrackKind::Audio {
            return Err(MediaRelayError::UnsupportedTrackKind {
                room_id: frame.room_id.clone(),
                user_id: frame.user_id.clone(),
                track_id: frame.track_id.clone(),
                kind: lookup.kind,
            });
        }

        let frames = self
            .relays
            .active_participants(&frame.room_id)?
            .into_iter()
            .filter(|participant| participant.user_id != frame.user_id)
            .filter(|participant| {
                participant
                    .tracks
                    .iter()
                    .any(|track| track.kind == MediaTrackKind::Audio)
            })
            .map(|participant| ProcessedAudioEgressFrame {
                recipient_id: participant.user_id,
                frame: frame.clone(),
            })
            .collect();

        Ok(frames)
    }
}
