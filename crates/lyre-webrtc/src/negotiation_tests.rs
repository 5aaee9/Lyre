use std::{net::IpAddr, sync::Arc};

use lyre_core::{RoomId, UserId};

use crate::{
    ServerMediaEgressError, ServerMediaIceCandidate, ServerMediaNegotiationError,
    ServerMediaNegotiator, ServerMediaOffer, ServerMediaProcessedAudioFrame, ServerMediaSessionKey,
    ServerMediaSessionRegistry, ServerMediaSessionState, WebRtcStack, SERVER_MEDIA_OPUS_FRAME_SIZE,
};

async fn offer_sdp() -> String {
    let offerer = WebRtcStack::new().create_peer_connection().await.unwrap();
    offerer.create_local_offer_for_test().await.unwrap()
}

fn offer(track: &str, sdp: String) -> ServerMediaOffer {
    ServerMediaOffer {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
        audio_track_id: track.to_owned(),
        sdp,
    }
}

fn host_candidate() -> ServerMediaIceCandidate {
    ServerMediaIceCandidate {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
        candidate: "candidate:1 1 UDP 2130706431 192.168.1.100 54321 typ host".to_owned(),
        sdp_mid: Some("0".to_owned()),
        sdp_mline_index: Some(0),
        username_fragment: None,
    }
}

async fn wait_for_local_candidates(
    negotiator: &ServerMediaNegotiator,
    key: &ServerMediaSessionKey,
) -> Vec<ServerMediaIceCandidate> {
    for _ in 0..128 {
        let candidates = negotiator.local_ice_candidates(key);
        if candidates
            .iter()
            .any(|candidate| candidate.candidate.starts_with("candidate:"))
            && candidates
                .iter()
                .any(|candidate| candidate.candidate.is_empty())
        {
            return candidates;
        }
        tokio::task::yield_now().await;
    }
    negotiator.local_ice_candidates(key)
}

#[tokio::test]
async fn answer_offer_marks_session_negotiating_and_stores_handle() {
    let sessions = Arc::new(ServerMediaSessionRegistry::new());
    let negotiator = ServerMediaNegotiator::new(WebRtcStack::new(), Arc::clone(&sessions));

    let answer = negotiator
        .answer_offer(offer("audio-main", offer_sdp().await))
        .await
        .unwrap();

    assert!(answer.sdp.starts_with("v=0"));
    assert_eq!(answer.state, ServerMediaSessionState::Negotiating);
    assert_eq!(
        sessions.active_sessions()[0].state,
        ServerMediaSessionState::Negotiating
    );
    assert_eq!(negotiator.stored_peer_connection_count(), 1);
}

#[tokio::test]
async fn failed_offer_does_not_create_session_or_handle() {
    let sessions = Arc::new(ServerMediaSessionRegistry::new());
    let negotiator = ServerMediaNegotiator::new(WebRtcStack::new(), Arc::clone(&sessions));

    let result = negotiator
        .answer_offer(offer("audio-main", "not sdp".to_owned()))
        .await;

    assert!(result.is_err());
    assert!(sessions.sessions().is_empty());
    assert_eq!(negotiator.stored_peer_connection_count(), 0);
}

#[tokio::test]
async fn repeated_successful_offer_replaces_track_and_handle() {
    let sessions = Arc::new(ServerMediaSessionRegistry::new());
    let negotiator = ServerMediaNegotiator::new(WebRtcStack::new(), Arc::clone(&sessions));
    let key = ServerMediaSessionKey {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
    };

    negotiator
        .answer_offer(offer("audio-main", offer_sdp().await))
        .await
        .unwrap();
    let first_handle = negotiator.stored_peer_connection_debug_id(&key).unwrap();
    negotiator
        .answer_offer(offer("audio-retry", offer_sdp().await))
        .await
        .unwrap();
    let second_handle = negotiator.stored_peer_connection_debug_id(&key).unwrap();

    assert_eq!(sessions.sessions().len(), 1);
    assert_eq!(sessions.sessions()[0].audio_track_id, "audio-retry");
    assert_eq!(negotiator.stored_peer_connection_count(), 1);
    assert_ne!(first_handle, second_handle);
}

#[tokio::test]
async fn failed_renegotiation_preserves_existing_session_and_handle() {
    let sessions = Arc::new(ServerMediaSessionRegistry::new());
    let negotiator = ServerMediaNegotiator::new(WebRtcStack::new(), Arc::clone(&sessions));
    let key = ServerMediaSessionKey {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
    };
    negotiator
        .answer_offer(offer("audio-main", offer_sdp().await))
        .await
        .unwrap();
    let status_before = sessions.sessions();
    let handle_before = negotiator.stored_peer_connection_debug_id(&key).unwrap();

    let result = negotiator
        .answer_offer(offer("audio-retry", "not sdp".to_owned()))
        .await;

    assert!(result.is_err());
    assert_eq!(sessions.sessions(), status_before);
    assert_eq!(
        negotiator.stored_peer_connection_debug_id(&key),
        Some(handle_before)
    );
    assert_eq!(negotiator.stored_peer_connection_count(), 1);
}

#[tokio::test]
async fn add_remote_ice_candidate_succeeds_for_existing_peer_without_state_change() {
    let sessions = Arc::new(ServerMediaSessionRegistry::new());
    let negotiator = ServerMediaNegotiator::new(WebRtcStack::new(), Arc::clone(&sessions));
    negotiator
        .answer_offer(offer("audio-main", offer_sdp().await))
        .await
        .unwrap();

    negotiator
        .add_remote_ice_candidate(host_candidate())
        .await
        .unwrap();

    assert_eq!(
        sessions.sessions()[0].state,
        ServerMediaSessionState::Negotiating
    );
}

#[tokio::test]
async fn add_remote_ice_candidate_missing_peer_returns_error_without_session() {
    let sessions = Arc::new(ServerMediaSessionRegistry::new());
    let negotiator = ServerMediaNegotiator::new(WebRtcStack::new(), Arc::clone(&sessions));

    let error = negotiator
        .add_remote_ice_candidate(host_candidate())
        .await
        .unwrap_err();

    assert!(matches!(error, ServerMediaNegotiationError::SessionMissing));
    assert!(sessions.sessions().is_empty());
}

#[tokio::test]
async fn session_status_returns_negotiated_audio_track_id() {
    let sessions = Arc::new(ServerMediaSessionRegistry::new());
    let negotiator = ServerMediaNegotiator::new(WebRtcStack::new(), Arc::clone(&sessions));
    let key = ServerMediaSessionKey {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
    };
    negotiator
        .answer_offer(offer("audio-main", offer_sdp().await))
        .await
        .unwrap();

    let status = negotiator.session_status(&key).unwrap();

    assert_eq!(status.audio_track_id, "audio-main");
    assert_eq!(status.state, ServerMediaSessionState::Negotiating);
}

#[tokio::test]
async fn local_ice_candidates_are_keyed_by_session() {
    let sessions = Arc::new(ServerMediaSessionRegistry::new());
    let negotiator = ServerMediaNegotiator::new(WebRtcStack::new(), Arc::clone(&sessions));
    let key = ServerMediaSessionKey {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
    };
    negotiator
        .answer_offer(offer("audio-main", offer_sdp().await))
        .await
        .unwrap();

    let candidates = wait_for_local_candidates(&negotiator, &key).await;

    assert!(candidates
        .iter()
        .any(|candidate| candidate.candidate.starts_with("candidate:")));
    assert!(candidates
        .iter()
        .any(|candidate| candidate.candidate.is_empty()));
    assert!(candidates
        .iter()
        .all(|candidate| candidate.room_id == key.room_id));
    assert!(candidates
        .iter()
        .all(|candidate| candidate.user_id == key.user_id));
}

#[tokio::test]
async fn local_ice_candidates_use_configured_server_media_public_ip() {
    let sessions = Arc::new(ServerMediaSessionRegistry::new());
    let public_ip = "203.0.113.10".parse::<IpAddr>().unwrap();
    let negotiator = ServerMediaNegotiator::new(
        WebRtcStack::with_server_media_public_ip(Some(public_ip)),
        Arc::clone(&sessions),
    );
    let key = ServerMediaSessionKey {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
    };
    negotiator
        .answer_offer(offer("audio-main", offer_sdp().await))
        .await
        .unwrap();

    let candidates = wait_for_local_candidates(&negotiator, &key).await;

    assert!(candidates
        .iter()
        .any(|candidate| candidate.candidate.contains(" 203.0.113.10 ")));
}

#[tokio::test]
async fn drain_pcm_frames_drains_existing_session_once() {
    let sessions = Arc::new(ServerMediaSessionRegistry::new());
    let negotiator = ServerMediaNegotiator::new(WebRtcStack::new(), Arc::clone(&sessions));
    let key = ServerMediaSessionKey {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
    };

    let test_offer = crate::test_support::server_media_offer_with_valid_opus_sender().await;
    let answer = negotiator
        .answer_offer(offer("audio-main", test_offer.offer_sdp.clone()))
        .await
        .unwrap();
    for candidate in test_offer.remote_candidates().await {
        negotiator
            .add_remote_ice_candidate(ServerMediaIceCandidate {
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
    test_offer
        .accept_answer_and_send_valid_opus(&answer, negotiator.local_ice_candidates(&key))
        .await;

    for _ in 0..100 {
        let frames = negotiator.drain_pcm_frames(&key);
        if let Some(frame) = frames.first() {
            assert_eq!(frame.track_id, "audio");
            assert_eq!(frame.sequence_number, 42);
            assert_eq!(frame.rtp_timestamp, 1234);
            assert_eq!(frame.sample_rate_hz, 48_000);
            assert_eq!(frame.channels, 1);
            assert_eq!(frame.samples.len(), SERVER_MEDIA_OPUS_FRAME_SIZE);
            assert!(negotiator.drain_pcm_frames(&key).is_empty());
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    panic!("negotiator did not drain decoded PCM from the existing session");
}

#[tokio::test]
async fn close_and_close_room_remove_stored_handles() {
    let sessions = Arc::new(ServerMediaSessionRegistry::new());
    let negotiator = ServerMediaNegotiator::new(WebRtcStack::new(), Arc::clone(&sessions));
    let key = ServerMediaSessionKey {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
    };
    negotiator
        .answer_offer(offer("audio-main", offer_sdp().await))
        .await
        .unwrap();

    negotiator.close(&key);

    assert_eq!(negotiator.stored_peer_connection_count(), 0);
    assert!(negotiator.local_ice_candidates(&key).is_empty());
    assert!(negotiator.remote_tracks(&key).is_empty());
    assert!(negotiator.received_rtp_packets(&key).is_empty());
    assert!(negotiator.drain_pcm_frames(&key).is_empty());
    assert!(negotiator.drain_decode_failures(&key).is_empty());

    negotiator
        .answer_offer(offer("audio-main", offer_sdp().await))
        .await
        .unwrap();
    negotiator.close_room(&RoomId::default_room());

    assert_eq!(negotiator.stored_peer_connection_count(), 0);
    assert!(negotiator.local_ice_candidates(&key).is_empty());
    assert!(negotiator.remote_tracks(&key).is_empty());
    assert!(negotiator.received_rtp_packets(&key).is_empty());
    assert!(negotiator.drain_pcm_frames(&key).is_empty());
    assert!(negotiator.drain_decode_failures(&key).is_empty());
}

#[tokio::test]
async fn send_processed_audio_frame_routes_to_existing_peer() {
    let sessions = Arc::new(ServerMediaSessionRegistry::new());
    let negotiator = ServerMediaNegotiator::new(WebRtcStack::new(), Arc::clone(&sessions));
    let key = ServerMediaSessionKey {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
    };

    negotiator
        .answer_offer(offer("audio-main", offer_sdp().await))
        .await
        .unwrap();

    let sent = negotiator
        .send_processed_audio_frame(
            &key,
            ServerMediaProcessedAudioFrame {
                sequence: 7,
                rtp_timestamp: None,
                sample_rate_hz: 48_000,
                channels: 1,
                samples: vec![0.1; SERVER_MEDIA_OPUS_FRAME_SIZE],
            },
        )
        .await
        .unwrap();

    assert_eq!(sent, 1);
}

#[tokio::test]
async fn send_processed_audio_frame_missing_peer_returns_context() {
    let sessions = Arc::new(ServerMediaSessionRegistry::new());
    let negotiator = ServerMediaNegotiator::new(WebRtcStack::new(), Arc::clone(&sessions));
    let key = ServerMediaSessionKey {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("missing"),
    };

    let error = negotiator
        .send_processed_audio_frame(
            &key,
            ServerMediaProcessedAudioFrame {
                sequence: 7,
                rtp_timestamp: None,
                sample_rate_hz: 48_000,
                channels: 1,
                samples: vec![0.1; SERVER_MEDIA_OPUS_FRAME_SIZE],
            },
        )
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        ServerMediaEgressError::PeerMissing { room_id, user_id }
            if room_id == key.room_id && user_id == key.user_id
    ));
}

#[tokio::test]
async fn server_media_snapshot_queries_return_empty_for_missing_session() {
    let sessions = Arc::new(ServerMediaSessionRegistry::new());
    let negotiator = ServerMediaNegotiator::new(WebRtcStack::new(), Arc::clone(&sessions));
    let missing_key = ServerMediaSessionKey {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("missing_user"),
    };

    assert!(negotiator.remote_tracks(&missing_key).is_empty());
    assert!(negotiator.received_rtp_packets(&missing_key).is_empty());
    assert!(negotiator.drain_pcm_frames(&missing_key).is_empty());
    assert!(negotiator.drain_decode_failures(&missing_key).is_empty());
}
