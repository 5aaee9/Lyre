use crate::{
    MediaRelayError, MediaRelayMode, MediaRelayRegistry, MediaRelayRegistryAggregate,
    MediaRelayStatus, MediaTrackKind, NoiseCancellationConfig, NoiseProvider,
    RegisterMediaTrackRequest, RoomId, StartMediaRelayRequest, StopMediaRelayRequest, UserId,
};

#[test]
fn default_status_is_inactive() {
    let registry = MediaRelayRegistry::new();
    let status = registry.status(RoomId::default_room());

    assert_eq!(status.status, MediaRelayStatus::Inactive);
    assert_eq!(status.mode, MediaRelayMode::MediaRelay);
    assert!(!status.server_side_audio_processing);
    assert!(!status.server_side_noise_cancelling);
    assert!(status.participants.is_empty());
}

#[test]
fn start_records_default_and_custom_noise() {
    let registry = MediaRelayRegistry::new();
    let default_started = registry.start(RoomId::default_room(), StartMediaRelayRequest::default());
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
fn read_only_track_lookup_does_not_create_unknown_room() {
    let registry = MediaRelayRegistry::new();
    let room_id = RoomId::parse_boundary("UNKNOWN").unwrap();

    assert_eq!(
        registry.require_track(&room_id, &UserId::from_external("user_01"), "audio-main"),
        Err(MediaRelayError::Inactive {
            room_id: room_id.clone(),
        })
    );
    assert!(!registry.contains_room(&room_id));
}

#[test]
fn read_only_track_lookup_reports_participant_track_and_kind() {
    let registry = MediaRelayRegistry::new();
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user_01");
    registry.start(
        room_id.clone(),
        StartMediaRelayRequest {
            noise: Some(NoiseCancellationConfig {
                provider: NoiseProvider::Rnnoise,
                intensity: 0.8,
                voice_activity_threshold: 0.2,
            }),
        },
    );

    assert_eq!(
        registry.require_track(&room_id, &user_id, "audio-main"),
        Err(MediaRelayError::ParticipantNotFound {
            room_id: room_id.clone(),
            user_id: user_id.clone(),
        })
    );

    registry
        .register_track(
            room_id.clone(),
            RegisterMediaTrackRequest {
                user_id: user_id.clone(),
                track_id: "audio-main".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();

    assert_eq!(
        registry.require_track(&room_id, &user_id, "missing-track"),
        Err(MediaRelayError::TrackNotFound {
            room_id: room_id.clone(),
            user_id: user_id.clone(),
            track_id: "missing-track".to_owned(),
        })
    );

    let track = registry
        .require_track(&room_id, &user_id, "audio-main")
        .unwrap();
    assert_eq!(track.kind, MediaTrackKind::Audio);
    assert_eq!(track.noise.provider, NoiseProvider::Rnnoise);
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
fn active_participants_requires_active_relay_without_creating_unknown_room() {
    let registry = MediaRelayRegistry::new();
    let room_id = RoomId::parse_boundary("UNKNOWN").unwrap();

    assert_eq!(
        registry.active_participants(&room_id),
        Err(MediaRelayError::Inactive {
            room_id: room_id.clone(),
        })
    );
    assert!(!registry.contains_room(&room_id));
}

#[test]
fn aggregate_counts_only_active_media_relays_without_creating_rooms() {
    let registry = MediaRelayRegistry::new();

    assert_eq!(
        registry.aggregate(),
        MediaRelayRegistryAggregate {
            active_rooms: 0,
            participants: 0,
        }
    );

    registry.start(RoomId::default_room(), StartMediaRelayRequest::default());
    registry
        .register_track(
            RoomId::default_room(),
            RegisterMediaTrackRequest {
                user_id: UserId::from_external("user_a"),
                track_id: "audio".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();

    assert_eq!(
        registry.aggregate(),
        MediaRelayRegistryAggregate {
            active_rooms: 1,
            participants: 1,
        }
    );
}

#[test]
fn active_participants_returns_sorted_participants_and_tracks() {
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

    let participants = registry.active_participants(&room_id).unwrap();

    assert_eq!(participants[0].user_id.as_str(), "user_a");
    assert_eq!(participants[0].tracks[0].track_id, "audio-main");
    assert_eq!(participants[0].tracks[1].track_id, "video-main");
    assert_eq!(participants[1].user_id.as_str(), "user_b");
}

#[test]
fn remove_participant_keeps_relay_active_noise_and_other_participants() {
    let registry = MediaRelayRegistry::new();
    let room_id = RoomId::default_room();
    let noise = NoiseCancellationConfig {
        provider: NoiseProvider::Rnnoise,
        intensity: 0.8,
        voice_activity_threshold: 0.2,
    };
    registry.start(
        room_id.clone(),
        StartMediaRelayRequest {
            noise: Some(noise.clone()),
        },
    );
    for (user_id, track_id) in [
        ("user_a", "audio-main"),
        ("user_a", "video-main"),
        ("user_b", "audio-main"),
    ] {
        registry
            .register_track(
                room_id.clone(),
                RegisterMediaTrackRequest {
                    user_id: UserId::from_external(user_id),
                    track_id: track_id.to_owned(),
                    kind: MediaTrackKind::Audio,
                },
            )
            .unwrap();
    }

    let status = registry
        .remove_participant(room_id, &UserId::from_external("user_a"))
        .unwrap();

    assert_eq!(status.status, MediaRelayStatus::Active);
    assert_eq!(status.mode, MediaRelayMode::MediaRelay);
    assert_eq!(status.noise, noise);
    assert_eq!(status.participants.len(), 1);
    assert_eq!(status.participants[0].user_id.as_str(), "user_b");
    assert_eq!(status.participants[0].tracks[0].track_id, "audio-main");
}

#[test]
fn remove_participant_requires_active_relay_without_creating_unknown_room() {
    let registry = MediaRelayRegistry::new();
    let missing_room = RoomId::parse_boundary("UNKNOWN").unwrap();

    assert_eq!(
        registry.remove_participant(missing_room.clone(), &UserId::from_external("user_a")),
        Err(MediaRelayError::Inactive {
            room_id: missing_room.clone(),
        })
    );
    assert!(!registry.contains_room(&missing_room));

    let room_id = RoomId::default_room();
    registry.status(room_id.clone());
    assert_eq!(
        registry.remove_participant(room_id.clone(), &UserId::from_external("user_a")),
        Err(MediaRelayError::Inactive { room_id })
    );
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
