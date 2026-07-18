use crate::api::AppState;
use lyre_core::{
    AudioFrame, MediaRelayError, MediaTrackKind, NoiseCancellationConfig, NoiseProvider,
    RegisterMediaTrackRequest, RoomId, StartMediaRelayRequest, StopMediaRelayRequest, UserId,
};
use lyre_noise_cancelling::{RNNOISE_CHANNELS, RNNOISE_FRAME_SIZE, RNNOISE_SAMPLE_RATE_HZ};
use lyre_webrtc::{ServerMediaOffer, ServerMediaSessionKey, WebRtcStack};

async fn offer_sdp() -> String {
    let offerer = WebRtcStack::new().create_peer_connection().await.unwrap();
    offerer.create_local_offer_for_test().await.unwrap()
}

fn frame(key: &ServerMediaSessionKey, track_id: &str, sequence: u64) -> AudioFrame {
    AudioFrame {
        room_id: key.room_id.clone(),
        user_id: key.user_id.clone(),
        track_id: track_id.to_owned(),
        sample_rate_hz: RNNOISE_SAMPLE_RATE_HZ,
        channels: RNNOISE_CHANNELS,
        sequence,
        rtp_timestamp: None,
        samples: (0..RNNOISE_FRAME_SIZE)
            .map(|index| ((index as f32) / 24.0).sin() * 0.1)
            .collect(),
    }
}

async fn answer_offer(state: &AppState, key: &ServerMediaSessionKey, track_id: &str) {
    state
        .answer_server_media_offer(ServerMediaOffer {
            room_id: key.room_id.clone(),
            user_id: key.user_id.clone(),
            audio_track_id: track_id.to_owned(),
            sdp: offer_sdp().await,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn replacing_audio_track_releases_old_track_processor_state() {
    let state = AppState::default();
    let key = ServerMediaSessionKey {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
    };
    state.start_media_relay(
        key.room_id.clone(),
        StartMediaRelayRequest {
            noise: Some(NoiseCancellationConfig {
                provider: NoiseProvider::Rnnoise,
                ..NoiseCancellationConfig::default()
            }),
        },
    );
    state
        .media_relays
        .register_track(
            key.room_id.clone(),
            RegisterMediaTrackRequest {
                user_id: key.user_id.clone(),
                track_id: "audio-old".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();
    answer_offer(&state, &key, "audio-old").await;
    let mut processed = state.subscribe_processed_media_frames(&key.room_id);

    state
        .process_media_frame(frame(&key, "audio-old", 1))
        .unwrap();
    let first = processed.recv().await.unwrap();
    state
        .process_media_frame(frame(&key, "audio-old", 2))
        .unwrap();
    let continued = processed.recv().await.unwrap();
    assert_ne!(continued.samples, first.samples);

    state
        .media_relays
        .register_track(
            key.room_id.clone(),
            RegisterMediaTrackRequest {
                user_id: key.user_id.clone(),
                track_id: "audio-new".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();
    answer_offer(&state, &key, "audio-new").await;
    state
        .process_media_frame(frame(&key, "audio-new", 3))
        .unwrap();
    let after_replacement = processed.recv().await.unwrap();

    assert_eq!(after_replacement.samples, first.samples);
}

#[tokio::test]
async fn closing_session_releases_processor_state_before_returning_relay_error() {
    let state = AppState::default();
    let key = ServerMediaSessionKey {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
    };
    state.start_media_relay(
        key.room_id.clone(),
        StartMediaRelayRequest {
            noise: Some(NoiseCancellationConfig {
                provider: NoiseProvider::Rnnoise,
                ..NoiseCancellationConfig::default()
            }),
        },
    );
    state
        .media_relays
        .register_track(
            key.room_id.clone(),
            RegisterMediaTrackRequest {
                user_id: key.user_id.clone(),
                track_id: "audio-main".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();
    let mut processed = state.subscribe_processed_media_frames(&key.room_id);
    state
        .process_media_frame(frame(&key, "audio-main", 1))
        .unwrap();
    let first = processed.recv().await.unwrap();
    state
        .process_media_frame(frame(&key, "audio-main", 2))
        .unwrap();
    let continued = processed.recv().await.unwrap();
    assert_ne!(continued.samples, first.samples);

    state.media_relays.stop(
        key.room_id.clone(),
        StopMediaRelayRequest {
            user_id: key.user_id.clone(),
        },
    );
    assert!(matches!(
        state.close_server_media_session_for_user(
            key.room_id.clone(),
            key.user_id.clone(),
        ).await,
        Err(MediaRelayError::Inactive { room_id }) if room_id == key.room_id
    ));

    state.start_media_relay(
        key.room_id.clone(),
        StartMediaRelayRequest {
            noise: Some(NoiseCancellationConfig {
                provider: NoiseProvider::Rnnoise,
                ..NoiseCancellationConfig::default()
            }),
        },
    );
    state
        .media_relays
        .register_track(
            key.room_id.clone(),
            RegisterMediaTrackRequest {
                user_id: key.user_id.clone(),
                track_id: "audio-main".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();
    state
        .process_media_frame(frame(&key, "audio-main", 3))
        .unwrap();
    let after_error = processed.recv().await.unwrap();

    assert_eq!(after_error.samples, first.samples);
}
