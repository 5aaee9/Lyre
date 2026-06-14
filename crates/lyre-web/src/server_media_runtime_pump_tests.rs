use crate::api::AppState;
use lyre_core::{
    MediaTrackKind, NoiseCancellationConfig, NoiseProvider, RegisterMediaTrackRequest, RoomId,
    StartMediaRelayRequest, StopMediaRelayRequest, UserId,
};
use lyre_webrtc::{ServerMediaIceCandidate, ServerMediaOffer, ServerMediaSessionKey, WebRtcStack};

async fn offer_sdp() -> String {
    let offerer = WebRtcStack::new().create_peer_connection().await.unwrap();
    offerer.create_local_offer_for_test().await.unwrap()
}

fn key() -> ServerMediaSessionKey {
    ServerMediaSessionKey {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
    }
}

async fn answer_offer(state: &AppState, track_id: &str) {
    let key = key();
    state
        .answer_server_media_offer(ServerMediaOffer {
            room_id: key.room_id,
            user_id: key.user_id,
            audio_track_id: track_id.to_owned(),
            sdp: offer_sdp().await,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn successful_offer_starts_and_replaces_runtime_pump() {
    let state = AppState::default();

    answer_offer(&state, "audio-main").await;
    assert_eq!(state.server_media_runtime_pump_count(), 1);
    answer_offer(&state, "audio-retry").await;
    assert_eq!(state.server_media_runtime_pump_count(), 1);
}

#[tokio::test]
async fn failed_offer_does_not_start_runtime_pump() {
    let state = AppState::default();
    let key = key();

    let result = state
        .answer_server_media_offer(ServerMediaOffer {
            room_id: key.room_id,
            user_id: key.user_id,
            audio_track_id: "audio-main".to_owned(),
            sdp: "not sdp".to_owned(),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(state.server_media_runtime_pump_count(), 0);
}

#[tokio::test]
async fn room_close_and_media_relay_stop_cancel_runtime_pumps() {
    let state = AppState::default();
    let key = key();

    answer_offer(&state, "audio-main").await;
    assert_eq!(state.server_media_runtime_pump_count(), 1);
    state.close_server_media_sessions_for_room(&RoomId::default_room());
    assert_eq!(state.server_media_runtime_pump_count(), 0);

    answer_offer(&state, "audio-main").await;
    assert_eq!(state.server_media_runtime_pump_count(), 1);
    state.stop_media_relay(
        key.room_id,
        StopMediaRelayRequest {
            user_id: key.user_id,
        },
    );
    assert_eq!(state.server_media_runtime_pump_count(), 0);
}

#[tokio::test]
async fn runtime_pump_processes_real_decoded_pcm_without_manual_drain() {
    let state = AppState::default();
    let key = key();
    state.media_relays.start(
        key.room_id.clone(),
        StartMediaRelayRequest {
            noise: Some(NoiseCancellationConfig {
                provider: NoiseProvider::Rnnoise,
                intensity: 0.5,
                voice_activity_threshold: 0.35,
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
        .answer_server_media_offer(ServerMediaOffer {
            room_id: key.room_id.clone(),
            user_id: key.user_id.clone(),
            audio_track_id: "audio-main".to_owned(),
            sdp: offer.offer_sdp.clone(),
        })
        .await
        .unwrap();
    for candidate in offer.remote_candidates().await {
        state
            .add_server_media_ice_candidate(ServerMediaIceCandidate {
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

    for _ in 0..150 {
        let frames = state.processed_media_frames(&key.room_id);
        if frames.iter().any(|frame| {
            frame.user_id == key.user_id
                && frame.track_id == "audio"
                && frame.sequence == 42
                && frame.noise.provider == NoiseProvider::Rnnoise
                && frame.samples.len() == lyre_webrtc::SERVER_MEDIA_OPUS_FRAME_SIZE
        }) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    panic!("server media runtime pump did not process decoded PCM");
}

#[tokio::test]
async fn runtime_pump_processes_after_delayed_relay_and_track_registration() {
    let state = AppState::default();
    let key = key();

    let offer = lyre_webrtc::test_support::server_media_offer_with_valid_opus_sender().await;
    let answer = state
        .answer_server_media_offer(ServerMediaOffer {
            room_id: key.room_id.clone(),
            user_id: key.user_id.clone(),
            audio_track_id: "audio-main".to_owned(),
            sdp: offer.offer_sdp.clone(),
        })
        .await
        .unwrap();
    for candidate in offer.remote_candidates().await {
        state
            .add_server_media_ice_candidate(ServerMediaIceCandidate {
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
    let connected = offer
        .accept_answer(&answer, state.server_media_ice_candidates(&key))
        .await;

    tokio::time::sleep(std::time::Duration::from_millis(60)).await;
    assert!(state.processed_media_frames(&key.room_id).is_empty());

    state
        .media_relays
        .start(key.room_id.clone(), StartMediaRelayRequest::default());
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
    connected.send_valid_opus_packets(100).await;

    for _ in 0..150 {
        if !state.processed_media_frames(&key.room_id).is_empty() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    panic!("server media runtime pump did not process after delayed relay registration");
}
