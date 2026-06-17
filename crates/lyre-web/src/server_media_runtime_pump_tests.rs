use crate::api::AppState;
use lyre_core::{
    MediaTrackKind, NoiseCancellationConfig, NoiseProvider, RegisterMediaTrackRequest, RoomId,
    StartMediaRelayRequest, StopMediaRelayRequest, UserId,
};
use lyre_webrtc::{
    ServerMediaIceCandidate, ServerMediaOffer, ServerMediaSessionKey, ServerMediaSessionState,
    WebRtcStack,
};
use tokio::time::{timeout, Duration};

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
async fn terminal_peer_failure_closes_session_and_runtime_pump() {
    let state = AppState::default();
    let key = key();

    answer_offer(&state, "audio-main").await;
    let peer = state.server_media_peer_connection_for_test(&key).unwrap();
    peer.close().await.unwrap();

    for _ in 0..100 {
        if state.server_media_runtime_pump_count() == 0 {
            assert_eq!(state.server_media_peer_connection_count(), 0);
            assert!(state.active_server_media_sessions().is_empty());
            assert_eq!(
                state.server_media_sessions()[0].state,
                ServerMediaSessionState::Closed
            );
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    panic!("terminal peer failure did not close server-media runtime state");
}

#[tokio::test]
async fn terminal_peer_failure_keeps_room_membership_and_relay_state() {
    let state = AppState::default();
    let key = key();
    let user = state
        .join_room_persisted(key.room_id.clone(), Default::default())
        .await
        .unwrap()
        .user;
    let key = ServerMediaSessionKey {
        room_id: key.room_id,
        user_id: user.id.clone(),
    };
    state.start_media_relay(
        key.room_id.clone(),
        StartMediaRelayRequest {
            noise: Some(NoiseCancellationConfig {
                provider: NoiseProvider::Dpdfnet,
                ..NoiseCancellationConfig::default()
            }),
        },
    );
    state
        .media_relays
        .register_track(
            key.room_id.clone(),
            RegisterMediaTrackRequest {
                user_id: user.id.clone(),
                track_id: "audio-main".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();
    state
        .answer_server_media_offer(ServerMediaOffer {
            room_id: key.room_id.clone(),
            user_id: user.id.clone(),
            audio_track_id: "audio-main".to_owned(),
            sdp: offer_sdp().await,
        })
        .await
        .unwrap();
    let peer = state.server_media_peer_connection_for_test(&key).unwrap();
    peer.close().await.unwrap();

    for _ in 0..100 {
        if state.server_media_runtime_pump_count() == 0 {
            assert!(state
                .registry
                .snapshot(key.room_id.clone())
                .users
                .iter()
                .any(|room_user| room_user.id == user.id));
            assert!(state
                .media_relays
                .status(key.room_id.clone())
                .participants
                .iter()
                .any(|participant| participant.user_id == user.id));
            assert_eq!(state.processed_audio_webrtc_egress_pump_count(), 1);
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    panic!("terminal peer failure did not close only server-media runtime state");
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
    let mut processed_frames = state.subscribe_processed_media_frames(&key.room_id);

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
        if let Ok(Ok(frame)) = timeout(Duration::from_millis(20), processed_frames.recv()).await {
            if frame.user_id == key.user_id
                && frame.track_id == "audio-main"
                && frame.sequence == 42
                && frame.noise.provider == NoiseProvider::Rnnoise
                && frame.samples.len() == lyre_webrtc::SERVER_MEDIA_OPUS_FRAME_SIZE
            {
                return;
            }
        }
    }

    panic!("server media runtime pump did not process decoded PCM");
}

#[tokio::test]
async fn runtime_pump_uses_negotiated_audio_track_id_for_decoded_pcm() {
    let state = AppState::default();
    let key = key();
    state
        .media_relays
        .start(key.room_id.clone(), StartMediaRelayRequest::default());
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
    let mut processed_frames = state.subscribe_processed_media_frames(&key.room_id);

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
        if let Ok(Ok(frame)) = timeout(Duration::from_millis(20), processed_frames.recv()).await {
            if frame.user_id == key.user_id
                && frame.track_id == "audio-main"
                && frame.sequence == 42
                && frame.samples.len() == lyre_webrtc::SERVER_MEDIA_OPUS_FRAME_SIZE
            {
                return;
            }
        }
    }

    panic!("server media runtime pump did not process decoded PCM under negotiated track id");
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
    let mut processed_frames = state.subscribe_processed_media_frames(&key.room_id);

    tokio::time::sleep(std::time::Duration::from_millis(60)).await;
    assert!(timeout(Duration::from_millis(25), processed_frames.recv())
        .await
        .is_err());

    state
        .media_relays
        .start(key.room_id.clone(), StartMediaRelayRequest::default());
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
    connected.send_valid_opus_packets(100).await;

    for _ in 0..150 {
        if timeout(Duration::from_millis(20), processed_frames.recv())
            .await
            .is_ok()
        {
            return;
        }
    }

    panic!("server media runtime pump did not process after delayed relay registration");
}
