use crate::{api::AppState, server_media_runtime};
use lyre_core::{
    MediaRelayError, MediaTrackKind, NoiseCancellationConfig, NoiseProvider,
    RegisterMediaTrackRequest, RoomId, StartMediaRelayRequest, UserId,
};
use lyre_webrtc::{ServerMediaPcmFrame, ServerMediaSessionKey};
use tokio::time::{timeout, Duration};

fn key() -> ServerMediaSessionKey {
    ServerMediaSessionKey {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
    }
}

fn pcm_frame(track_id: impl Into<String>, sequence_number: u16) -> ServerMediaPcmFrame {
    ServerMediaPcmFrame {
        track_id: track_id.into(),
        sequence_number,
        rtp_timestamp: 48_000,
        sample_rate_hz: 48_000,
        channels: 1,
        samples: vec![0.25; 960],
    }
}

fn start_relay_with_audio_track(state: &AppState) {
    let key = key();
    state
        .media_relays
        .start(key.room_id.clone(), StartMediaRelayRequest::default());
    state
        .media_relays
        .register_track(
            key.room_id,
            RegisterMediaTrackRequest {
                user_id: key.user_id,
                track_id: "audio-main".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();
}

#[tokio::test]
async fn app_state_processes_server_media_pcm_frame_into_runtime() {
    let state = AppState::default();
    let key = key();
    start_relay_with_audio_track(&state);
    let mut frames = state.subscribe_processed_media_frames(&key.room_id);

    assert_eq!(
        state.process_server_media_pcm_frame(&key, pcm_frame("audio-main", 7)),
        Ok(())
    );

    let frame = timeout(Duration::from_millis(100), frames.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(frame.room_id, key.room_id);
    assert_eq!(frame.user_id, key.user_id);
    assert_eq!(frame.track_id, "audio-main");
    assert_eq!(frame.sequence, 7);
    assert_eq!(frame.sample_rate_hz, 48_000);
    assert_eq!(frame.channels, 1);
    assert_eq!(frame.rtp_timestamp, Some(48_000));
    assert_eq!(frame.samples.len(), 960);
    assert_eq!(frame.noise.provider, NoiseProvider::Off);
}

#[test]
fn app_state_process_server_media_pcm_frame_returns_relay_error() {
    let state = AppState::default();
    let key = key();

    assert_eq!(
        state.process_server_media_pcm_frame(&key, pcm_frame("audio-main", 7)),
        Err(MediaRelayError::Inactive {
            room_id: key.room_id,
        })
    );
}

#[test]
fn server_media_runtime_batch_stops_on_first_error_without_processing_later_frames() {
    let state = AppState::default();
    let key = key();

    assert_eq!(
        server_media_runtime::process_pcm_frame_batch(
            &state.media_runtime,
            &key,
            vec![pcm_frame("missing-track", 7), pcm_frame("audio-main", 8)],
        ),
        Err(MediaRelayError::Inactive {
            room_id: key.room_id.clone(),
        })
    );

    start_relay_with_audio_track(&state);

    assert_eq!(
        server_media_runtime::process_pcm_frame_batch(
            &state.media_runtime,
            &key,
            vec![pcm_frame("missing-track", 9), pcm_frame("audio-main", 10)],
        ),
        Err(MediaRelayError::TrackNotFound {
            room_id: key.room_id.clone(),
            user_id: key.user_id.clone(),
            track_id: "missing-track".to_owned(),
        })
    );
}

#[tokio::test]
async fn app_state_processes_real_drained_server_media_pcm_batch() {
    let state = AppState::default();
    let key = key();
    state.media_relays.start(
        key.room_id.clone(),
        StartMediaRelayRequest {
            noise: Some(NoiseCancellationConfig {
                provider: NoiseProvider::Rnnoise,
                intensity: 0.5,
                voice_activity_threshold: 0.35,
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
                track_id: "audio".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();

    let offer = lyre_webrtc::test_support::server_media_offer_with_valid_opus_sender().await;
    let answer = state
        .server_media_negotiator
        .answer_offer(lyre_webrtc::ServerMediaOffer {
            room_id: key.room_id.clone(),
            user_id: key.user_id.clone(),
            audio_track_id: "audio-main".to_owned(),
            sdp: offer.offer_sdp.clone(),
        })
        .await
        .unwrap();
    for candidate in offer.remote_candidates().await {
        state
            .server_media_negotiator
            .add_remote_ice_candidate(lyre_webrtc::ServerMediaIceCandidate {
                room_id: key.room_id.clone(),
                user_id: key.user_id.clone(),
                candidate: candidate.candidate,
                sdp_mid: candidate.sdp_mid,
                sdp_mline_index: candidate.sdp_mline_index,
                username_fragment: candidate.username_fragment,
            })
            .await
            .unwrap();
    }
    offer
        .accept_answer_and_send_valid_opus(&answer, state.server_media_ice_candidates(&key))
        .await;

    let decoded_frames = loop {
        let frames = state.drain_server_media_pcm_frames(&key);
        if !frames.is_empty() {
            break frames;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    };
    let decoded_samples = decoded_frames[0].samples.clone();
    let mut processed_frames = state.subscribe_processed_media_frames(&key.room_id);
    let processed =
        server_media_runtime::process_pcm_frame_batch(&state.media_runtime, &key, decoded_frames)
            .unwrap();

    assert!(processed > 0);
    for _ in 0..processed {
        let frame = timeout(Duration::from_millis(100), processed_frames.recv())
            .await
            .unwrap()
            .unwrap();
        if frame.user_id == key.user_id
            && frame.track_id == "audio"
            && frame.sequence == 42
            && frame.sample_rate_hz == 48_000
            && frame.channels == 1
            && frame.noise.provider == NoiseProvider::Rnnoise
            && frame.samples.len() == lyre_webrtc::SERVER_MEDIA_OPUS_FRAME_SIZE
            && frame.samples != decoded_samples
        {
            assert_eq!(state.process_server_media_pcm_frames(&key), Ok(0));
            return;
        }
    }
    panic!("decoded server media PCM frame was not processed with RNNoise");
}

#[tokio::test]
async fn app_state_processes_drained_server_media_pcm_under_negotiated_track_id() {
    let state = AppState::default();
    let key = key();
    start_relay_with_audio_track(&state);

    let offer = lyre_webrtc::test_support::server_media_offer_with_valid_opus_sender().await;
    let answer = state
        .server_media_negotiator
        .answer_offer(lyre_webrtc::ServerMediaOffer {
            room_id: key.room_id.clone(),
            user_id: key.user_id.clone(),
            audio_track_id: "audio-main".to_owned(),
            sdp: offer.offer_sdp.clone(),
        })
        .await
        .unwrap();
    for candidate in offer.remote_candidates().await {
        state
            .server_media_negotiator
            .add_remote_ice_candidate(lyre_webrtc::ServerMediaIceCandidate {
                room_id: key.room_id.clone(),
                user_id: key.user_id.clone(),
                candidate: candidate.candidate,
                sdp_mid: candidate.sdp_mid,
                sdp_mline_index: candidate.sdp_mline_index,
                username_fragment: candidate.username_fragment,
            })
            .await
            .unwrap();
    }
    offer
        .accept_answer_and_send_valid_opus(&answer, state.server_media_ice_candidates(&key))
        .await;

    let mut processed_frames = state.subscribe_processed_media_frames(&key.room_id);
    for _ in 0..100 {
        if state.process_server_media_pcm_frames(&key).unwrap() > 0 {
            let frame = timeout(Duration::from_millis(100), processed_frames.recv())
                .await
                .unwrap()
                .unwrap();
            if frame.user_id == key.user_id
                && frame.track_id == "audio-main"
                && frame.sequence == 42
            {
                return;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    panic!("decoded server media PCM frame was not processed under negotiated track id");
}

#[tokio::test]
async fn app_state_discards_real_drained_server_media_pcm_batch_on_error() {
    let state = AppState::default();
    let key = key();

    let offer = lyre_webrtc::test_support::server_media_offer_with_valid_opus_sender().await;
    let answer = state
        .server_media_negotiator
        .answer_offer(lyre_webrtc::ServerMediaOffer {
            room_id: key.room_id.clone(),
            user_id: key.user_id.clone(),
            audio_track_id: "audio-main".to_owned(),
            sdp: offer.offer_sdp.clone(),
        })
        .await
        .unwrap();
    for candidate in offer.remote_candidates().await {
        state
            .server_media_negotiator
            .add_remote_ice_candidate(lyre_webrtc::ServerMediaIceCandidate {
                room_id: key.room_id.clone(),
                user_id: key.user_id.clone(),
                candidate: candidate.candidate,
                sdp_mid: candidate.sdp_mid,
                sdp_mline_index: candidate.sdp_mline_index,
                username_fragment: candidate.username_fragment,
            })
            .await
            .unwrap();
    }
    offer
        .accept_answer_and_send_valid_opus(&answer, state.server_media_ice_candidates(&key))
        .await;

    for _ in 0..100 {
        match state.process_server_media_pcm_frames(&key) {
            Err(error) => {
                assert_eq!(
                    error,
                    MediaRelayError::Inactive {
                        room_id: key.room_id.clone(),
                    }
                );
                assert_eq!(state.process_server_media_pcm_frames(&key), Ok(0));
                return;
            }
            Ok(0) => tokio::time::sleep(std::time::Duration::from_millis(20)).await,
            Ok(count) => panic!("processed {count} frames without an active relay"),
        }
    }

    panic!("decoded server media PCM frame was not drained by AppState");
}
