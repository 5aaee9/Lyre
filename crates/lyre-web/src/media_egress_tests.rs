use crate::{api::AppState, media_egress::ProcessedAudioEgressFanout};
use lyre_core::{
    MediaRelayError, MediaRelayRegistry, MediaTrackKind, NoiseCancellationConfig,
    ProcessedAudioFrame, RegisterMediaTrackRequest, RoomId, StartMediaRelayRequest,
    StopMediaRelayRequest, UserId,
};
use std::sync::Arc;

fn frame(room_id: RoomId, user_id: UserId, track_id: impl Into<String>) -> ProcessedAudioFrame {
    ProcessedAudioFrame {
        room_id,
        user_id,
        track_id: track_id.into(),
        sample_rate_hz: 48_000,
        channels: 1,
        sequence: 7,
        rtp_timestamp: None,
        samples: vec![0.1, -0.2, 0.3],
        noise: NoiseCancellationConfig::default(),
    }
}

fn register(
    relays: &MediaRelayRegistry,
    room_id: &RoomId,
    user_id: &str,
    track_id: &str,
    kind: MediaTrackKind,
) {
    relays
        .register_track(
            room_id.clone(),
            RegisterMediaTrackRequest {
                user_id: UserId::from_external(user_id),
                track_id: track_id.to_owned(),
                kind,
            },
        )
        .unwrap();
}

#[test]
fn fanout_excludes_source_and_sorts_audio_recipients() {
    let relays = Arc::new(MediaRelayRegistry::new());
    let fanout = ProcessedAudioEgressFanout::new(Arc::clone(&relays));
    let room_id = RoomId::default_room();
    relays.start(room_id.clone(), StartMediaRelayRequest::default());
    register(
        &relays,
        &room_id,
        "source",
        "audio-main",
        MediaTrackKind::Audio,
    );
    register(
        &relays,
        &room_id,
        "user_c",
        "audio-main",
        MediaTrackKind::Audio,
    );
    register(
        &relays,
        &room_id,
        "user_a",
        "audio-main",
        MediaTrackKind::Audio,
    );
    register(&relays, &room_id, "user_b", "camera", MediaTrackKind::Video);

    let frames = fanout
        .fanout(&frame(
            room_id,
            UserId::from_external("source"),
            "audio-main",
        ))
        .unwrap();

    assert_eq!(
        frames
            .iter()
            .map(|frame| frame.recipient_id.as_str())
            .collect::<Vec<_>>(),
        vec!["user_a", "user_c"]
    );
    assert!(frames.iter().all(|egress| egress.frame.sequence == 7));
}

#[test]
fn fanout_sends_each_recipient_once_when_they_have_multiple_audio_tracks() {
    let relays = Arc::new(MediaRelayRegistry::new());
    let fanout = ProcessedAudioEgressFanout::new(Arc::clone(&relays));
    let room_id = RoomId::default_room();
    relays.start(room_id.clone(), StartMediaRelayRequest::default());
    register(
        &relays,
        &room_id,
        "source",
        "audio-main",
        MediaTrackKind::Audio,
    );
    register(
        &relays,
        &room_id,
        "user_a",
        "audio-main",
        MediaTrackKind::Audio,
    );
    register(
        &relays,
        &room_id,
        "user_a",
        "audio-alt",
        MediaTrackKind::Audio,
    );

    let frames = fanout
        .fanout(&frame(
            room_id,
            UserId::from_external("source"),
            "audio-main",
        ))
        .unwrap();

    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].recipient_id.as_str(), "user_a");
}

#[test]
fn fanout_excludes_video_only_participants() {
    let relays = Arc::new(MediaRelayRegistry::new());
    let fanout = ProcessedAudioEgressFanout::new(Arc::clone(&relays));
    let room_id = RoomId::default_room();
    relays.start(room_id.clone(), StartMediaRelayRequest::default());
    register(
        &relays,
        &room_id,
        "source",
        "audio-main",
        MediaTrackKind::Audio,
    );
    register(&relays, &room_id, "video", "camera", MediaTrackKind::Video);

    let frames = fanout
        .fanout(&frame(
            room_id,
            UserId::from_external("source"),
            "audio-main",
        ))
        .unwrap();

    assert!(frames.is_empty());
}

#[test]
fn fanout_propagates_source_validation_errors() {
    let relays = Arc::new(MediaRelayRegistry::new());
    let fanout = ProcessedAudioEgressFanout::new(Arc::clone(&relays));
    let room_id = RoomId::default_room();
    let source = UserId::from_external("source");

    assert_eq!(
        fanout.fanout(&frame(room_id.clone(), source.clone(), "audio-main")),
        Err(MediaRelayError::Inactive {
            room_id: room_id.clone(),
        })
    );
    assert!(!relays.contains_room(&room_id));

    relays.start(room_id.clone(), StartMediaRelayRequest::default());
    assert_eq!(
        fanout.fanout(&frame(room_id.clone(), source.clone(), "audio-main")),
        Err(MediaRelayError::ParticipantNotFound {
            room_id: room_id.clone(),
            user_id: source.clone(),
        })
    );

    register(&relays, &room_id, "source", "other", MediaTrackKind::Audio);
    assert_eq!(
        fanout.fanout(&frame(room_id.clone(), source.clone(), "audio-main")),
        Err(MediaRelayError::TrackNotFound {
            room_id: room_id.clone(),
            user_id: source.clone(),
            track_id: "audio-main".to_owned(),
        })
    );
}

#[test]
fn fanout_rejects_non_audio_source_track() {
    let relays = Arc::new(MediaRelayRegistry::new());
    let fanout = ProcessedAudioEgressFanout::new(Arc::clone(&relays));
    let room_id = RoomId::default_room();
    let source = UserId::from_external("source");
    relays.start(room_id.clone(), StartMediaRelayRequest::default());
    register(&relays, &room_id, "source", "camera", MediaTrackKind::Video);

    assert_eq!(
        fanout.fanout(&frame(room_id.clone(), source.clone(), "camera")),
        Err(MediaRelayError::UnsupportedTrackKind {
            room_id,
            user_id: source,
            track_id: "camera".to_owned(),
            kind: MediaTrackKind::Video,
        })
    );
}

#[test]
fn fanout_rejects_stopped_relay() {
    let relays = Arc::new(MediaRelayRegistry::new());
    let fanout = ProcessedAudioEgressFanout::new(Arc::clone(&relays));
    let room_id = RoomId::default_room();
    let source = UserId::from_external("source");
    relays.start(room_id.clone(), StartMediaRelayRequest::default());
    register(
        &relays,
        &room_id,
        "source",
        "audio-main",
        MediaTrackKind::Audio,
    );
    relays.stop(
        room_id.clone(),
        StopMediaRelayRequest {
            user_id: source.clone(),
        },
    );

    assert_eq!(
        fanout.fanout(&frame(room_id.clone(), source, "audio-main")),
        Err(MediaRelayError::Inactive { room_id })
    );
}

#[test]
fn app_state_processed_audio_egress_uses_shared_registry() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    state
        .media_relays
        .start(room_id.clone(), StartMediaRelayRequest::default());
    register(
        &state.media_relays,
        &room_id,
        "source",
        "audio-main",
        MediaTrackKind::Audio,
    );
    register(
        &state.media_relays,
        &room_id,
        "recipient",
        "audio-main",
        MediaTrackKind::Audio,
    );

    let frames = state
        .processed_audio_egress_frames(&frame(
            room_id,
            UserId::from_external("source"),
            "audio-main",
        ))
        .unwrap();

    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].recipient_id.as_str(), "recipient");
}
