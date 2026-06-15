use crate::api::AppState;
use lyre_core::{
    AudioFrame, MediaTrackKind, RegisterMediaTrackRequest, RoomId, StartMediaRelayRequest,
    StopMediaRelayRequest, UserId,
};
use lyre_webrtc::{ServerMediaIceCandidate, ServerMediaOffer, ServerMediaSessionKey, WebRtcStack};

#[tokio::test]
async fn app_state_start_and_stop_manage_egress_pump() {
    let state = AppState::default();
    let room_id = RoomId::default_room();

    state.start_media_relay(room_id.clone(), StartMediaRelayRequest::default());
    assert_eq!(state.processed_audio_webrtc_egress_pump_count(), 1);
    state.start_media_relay(room_id.clone(), StartMediaRelayRequest::default());
    assert_eq!(state.processed_audio_webrtc_egress_pump_count(), 1);

    state.stop_media_relay(
        room_id,
        StopMediaRelayRequest {
            user_id: UserId::from_external("user_01"),
        },
    );
    assert_eq!(state.processed_audio_webrtc_egress_pump_count(), 0);
}

#[tokio::test]
async fn close_server_media_sessions_stops_egress_pump() {
    let state = AppState::default();
    let room_id = RoomId::default_room();

    state.start_media_relay(room_id.clone(), StartMediaRelayRequest::default());
    assert_eq!(state.processed_audio_webrtc_egress_pump_count(), 1);
    state.close_server_media_sessions_for_room(&room_id);
    assert_eq!(state.processed_audio_webrtc_egress_pump_count(), 0);
}

fn audio_frame(room_id: RoomId, user_id: UserId) -> AudioFrame {
    AudioFrame {
        room_id,
        user_id,
        track_id: "audio-main".to_owned(),
        sample_rate_hz: 48_000,
        channels: 1,
        sequence: 1,
        samples: vec![0.1; lyre_webrtc::SERVER_MEDIA_OPUS_FRAME_SIZE],
    }
}

fn register_audio_track(state: &AppState, room_id: &RoomId, user_id: &str) {
    state
        .media_relays
        .register_track(
            room_id.clone(),
            RegisterMediaTrackRequest {
                user_id: UserId::from_external(user_id),
                track_id: "audio-main".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();
}

async fn answer_offer(state: &AppState, room_id: &RoomId, user_id: &str) -> ServerMediaSessionKey {
    let offerer = WebRtcStack::new().create_peer_connection().await.unwrap();
    let key = ServerMediaSessionKey {
        room_id: room_id.clone(),
        user_id: UserId::from_external(user_id),
    };
    state
        .answer_server_media_offer(ServerMediaOffer {
            room_id: key.room_id.clone(),
            user_id: key.user_id.clone(),
            audio_track_id: "audio-main".to_owned(),
            sdp: offerer.create_local_offer_for_test().await.unwrap(),
        })
        .await
        .unwrap();
    key
}

async fn connect_test_offer(
    state: &AppState,
    room_id: &RoomId,
    user_id: &str,
) -> lyre_webrtc::test_support::ServerMediaConnectedOffer {
    let key = ServerMediaSessionKey {
        room_id: room_id.clone(),
        user_id: UserId::from_external(user_id),
    };
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
        .accept_answer(&answer, state.server_media_ice_candidates(&key))
        .await
}

#[tokio::test]
async fn processed_audio_frame_is_sent_to_recipient_server_media_peer() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    state.start_media_relay(room_id.clone(), StartMediaRelayRequest::default());
    register_audio_track(&state, &room_id, "source");
    register_audio_track(&state, &room_id, "recipient");
    let source_key = answer_offer(&state, &room_id, "source").await;
    let recipient_key = answer_offer(&state, &room_id, "recipient").await;

    state
        .process_media_frame(audio_frame(
            room_id.clone(),
            UserId::from_external("source"),
        ))
        .unwrap();

    for _ in 0..100 {
        let recipient = state
            .server_media_peer_connection_for_test(&recipient_key)
            .unwrap();
        if !recipient.sent_egress_rtp_packets_for_test().is_empty() {
            let source = state
                .server_media_peer_connection_for_test(&source_key)
                .unwrap();
            assert!(source.sent_egress_rtp_packets_for_test().is_empty());
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    panic!("processed audio frame was not sent to recipient server-media peer");
}

#[tokio::test]
async fn server_relay_audio_reaches_recipient_peer_connection() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    state.start_media_relay(room_id.clone(), StartMediaRelayRequest::default());
    register_audio_track(&state, &room_id, "source");
    register_audio_track(&state, &room_id, "recipient");
    let source = connect_test_offer(&state, &room_id, "source").await;
    let recipient = connect_test_offer(&state, &room_id, "recipient").await;

    source.send_valid_opus_packets(100).await;

    for _ in 0..150 {
        if !recipient.received_remote_rtp_packets().is_empty() {
            assert!(source.received_remote_rtp_packets().is_empty());
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    panic!("server relay audio RTP did not reach recipient peer connection");
}

#[tokio::test]
async fn server_relay_audio_survives_repeated_room_relay_start() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    state.start_media_relay(room_id.clone(), StartMediaRelayRequest::default());
    register_audio_track(&state, &room_id, "source");
    let source = connect_test_offer(&state, &room_id, "source").await;

    state.start_media_relay(room_id.clone(), StartMediaRelayRequest::default());
    register_audio_track(&state, &room_id, "recipient");
    let recipient = connect_test_offer(&state, &room_id, "recipient").await;

    source.send_valid_opus_packets(100).await;

    for _ in 0..150 {
        if !recipient.received_remote_rtp_packets().is_empty() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    panic!("server relay audio RTP did not reach recipient after repeated relay start");
}
