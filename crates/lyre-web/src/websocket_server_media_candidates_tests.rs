use crate::{
    api::{handle_signal_message, AppState},
    signalling::{SignalMessage, SignalPayload},
};
use lyre_core::{RoomId, UserId};
use lyre_webrtc::{ServerMediaOffer, WebRtcStack};

fn room_id() -> RoomId {
    RoomId::default_room()
}

fn test_user_id(value: &str) -> UserId {
    UserId::from_external(value)
}

async fn offer_sdp() -> String {
    let offerer = WebRtcStack::new().create_peer_connection().await.unwrap();
    offerer.create_local_offer_for_test().await.unwrap()
}

fn host_candidate_payload() -> SignalPayload {
    SignalPayload::ServerMediaIceCandidate {
        candidate: "candidate:1 1 UDP 2130706431 192.168.1.100 54321 typ host".into(),
        sdp_mid: Some("0".into()),
        sdp_mline_index: Some(0),
        username_fragment: Some("ufrag".into()),
    }
}

#[tokio::test]
async fn server_media_candidates_request_returns_candidates_to_same_socket() {
    let state = AppState::default();
    let room_id = room_id();
    let user_id = test_user_id("user_a");
    let peer_id = test_user_id("peer");
    let (user_tx, _user_rx) = tokio::sync::mpsc::unbounded_channel();
    let (peer_tx, mut peer_rx) = tokio::sync::mpsc::unbounded_channel();
    state
        .peers
        .connect(&state.registry, room_id.clone(), user_id.clone(), user_tx);
    state
        .peers
        .connect(&state.registry, room_id.clone(), peer_id, peer_tx);

    let response = handle_signal_message(
        &state,
        &room_id,
        &user_id,
        SignalMessage::to_self(
            room_id.clone(),
            user_id.clone(),
            SignalPayload::ServerMediaIceCandidatesRequest,
        ),
    )
    .await
    .unwrap();

    assert_eq!(response.recipient_id, Some(user_id));
    assert!(matches!(
        response.payload,
        SignalPayload::ServerMediaIceCandidates { .. }
    ));
    assert!(peer_rx.try_recv().is_err());
}

#[tokio::test]
async fn server_media_candidate_without_session_returns_error_signal() {
    let state = AppState::default();
    let room_id = room_id();
    let user_id = test_user_id("user_a");

    let response = handle_signal_message(
        &state,
        &room_id,
        &user_id,
        SignalMessage::to_self(room_id.clone(), user_id.clone(), host_candidate_payload()),
    )
    .await
    .unwrap();

    assert_eq!(response.sender_id, user_id);
    let SignalPayload::Error { message } = response.payload else {
        panic!("expected error response");
    };
    let message = message.to_lowercase();
    assert!(
        message.contains("missing")
            || message.contains("unavailable")
            || message.contains("disappeared")
    );
    assert!(message.contains("session"));
}

#[tokio::test]
async fn server_media_candidate_with_session_is_accepted_over_websocket() {
    let state = AppState::default();
    let room_id = room_id();
    let user_id = test_user_id("user_a");
    state
        .answer_server_media_offer(ServerMediaOffer {
            room_id: room_id.clone(),
            user_id: user_id.clone(),
            audio_track_id: "audio-main".into(),
            sdp: offer_sdp().await,
        })
        .await
        .unwrap();

    let response = handle_signal_message(
        &state,
        &room_id,
        &user_id,
        SignalMessage::to_self(room_id.clone(), user_id.clone(), host_candidate_payload()),
    )
    .await;

    assert!(response.is_none());
}
