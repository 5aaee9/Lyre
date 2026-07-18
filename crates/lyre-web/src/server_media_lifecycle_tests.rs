use crate::{api::AppState, error::ApiError};
use lyre_core::{
    MediaTrackKind, RegisterMediaTrackRequest, RoomId, StartMediaRelayRequest, UserId,
};
use lyre_webrtc::{ServerMediaOffer, WebRtcStack};

async fn offer_sdp() -> String {
    let offerer = WebRtcStack::new().create_peer_connection().await.unwrap();
    offerer.create_local_offer_for_test().await.unwrap()
}

#[tokio::test]
async fn close_waiting_during_offer_commit_removes_the_committed_session_and_pump() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user_01");
    state
        .media_relays
        .start(room_id.clone(), StartMediaRelayRequest::default());
    state
        .media_relays
        .register_track(
            room_id.clone(),
            RegisterMediaTrackRequest {
                user_id: user_id.clone(),
                track_id: "audio-main".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();
    state.pause_next_server_media_offer_before_pump_for_test();

    let offer_state = state.clone();
    let offer_room_id = room_id.clone();
    let offer_user_id = user_id.clone();
    let offer = tokio::spawn(async move {
        offer_state
            .answer_server_media_offer(ServerMediaOffer {
                room_id: offer_room_id,
                user_id: offer_user_id,
                audio_track_id: "audio-main".to_owned(),
                sdp: offer_sdp().await,
            })
            .await
    });
    state
        .wait_for_server_media_offer_before_pump_for_test()
        .await;

    let close_state = state.clone();
    let close_room_id = room_id.clone();
    let close_user_id = user_id.clone();
    let close = tokio::spawn(async move {
        close_state
            .close_server_media_session_for_user(close_room_id, close_user_id)
            .await
    });
    tokio::task::yield_now().await;
    assert!(
        !close.is_finished(),
        "close must wait for the in-flight offer lifecycle"
    );

    state.resume_server_media_offer_for_test();
    offer.await.unwrap().unwrap();
    close.await.unwrap().unwrap();

    assert_eq!(state.server_media_peer_connection_count(), 0);
    assert_eq!(state.server_media_runtime_pump_count(), 0);
    assert!(state.server_media_sessions().is_empty());
}

#[tokio::test]
async fn leave_queued_before_authorized_offer_prevents_a_ghost_media_session() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let joined = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap();
    state
        .media_relays
        .start(room_id.clone(), StartMediaRelayRequest::default());
    state
        .media_relays
        .register_track(
            room_id.clone(),
            RegisterMediaTrackRequest {
                user_id: joined.user.id.clone(),
                track_id: "audio-main".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();
    let lifecycle_guard = state.server_media_lifecycle.lock(&room_id).await;

    let leave_state = state.clone();
    let leave_room_id = room_id.clone();
    let leave_user_id = joined.user.id.clone();
    let leave = tokio::spawn(async move {
        leave_state
            .leave_room_persisted(&leave_room_id, &leave_user_id)
            .await
    });
    for _ in 0..100 {
        if state
            .registry
            .validate_access_token(&room_id, &joined.user.id, &joined.access_token)
            .is_err()
        {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(state
        .registry
        .validate_access_token(&room_id, &joined.user.id, &joined.access_token)
        .is_err());

    let offer_state = state.clone();
    let offer_room_id = room_id.clone();
    let offer_user_id = joined.user.id.clone();
    let access_token = joined.access_token.clone();
    let offer = tokio::spawn(async move {
        offer_state
            .answer_authorized_server_media_offer_with_subscriptions(
                ServerMediaOffer {
                    room_id: offer_room_id,
                    user_id: offer_user_id,
                    audio_track_id: "audio-main".to_owned(),
                    sdp: "authorization must fail before SDP parsing".to_owned(),
                },
                &access_token,
            )
            .await
    });
    tokio::task::yield_now().await;
    drop(lifecycle_guard);

    assert!(leave.await.unwrap().unwrap().removed);
    assert!(matches!(offer.await.unwrap(), Err(ApiError::Unauthorized)));
    assert_eq!(state.server_media_peer_connection_count(), 0);
    assert_eq!(state.server_media_runtime_pump_count(), 0);
    assert!(state.server_media_sessions().is_empty());
}
