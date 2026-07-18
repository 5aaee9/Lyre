use crate::{api::AppState, error::ApiError};
use lyre_core::{
    MediaRelayStatus, MediaTrackKind, RegisterMediaTrackRequest, RoomAccessToken, RoomId,
    StartMediaRelayRequest, StopMediaRelayRequest, UpdateMediaRelaySettingsRequest, UserId,
};
use lyre_webrtc::ServerMediaIceCandidate;

async fn wait_for_access_revocation(
    state: &AppState,
    room_id: &RoomId,
    user_id: &UserId,
    access_token: &RoomAccessToken,
) {
    for _ in 0..100 {
        if state
            .registry
            .validate_access_token(room_id, user_id, access_token)
            .is_err()
        {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("leave did not revoke room access before the control mutation was queued");
}

#[tokio::test]
async fn revoked_member_cannot_restart_relay_after_leave() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let joined = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
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
    wait_for_access_revocation(&state, &room_id, &joined.user.id, &joined.access_token).await;

    let start_state = state.clone();
    let start_room_id = room_id.clone();
    let access_token = joined.access_token.clone();
    let start = tokio::spawn(async move {
        start_state
            .start_authorized_media_relay(
                start_room_id,
                StartMediaRelayRequest::default(),
                &access_token,
            )
            .await
    });
    drop(lifecycle_guard);

    assert!(leave.await.unwrap().unwrap().removed);
    assert!(matches!(start.await.unwrap(), Err(ApiError::Unauthorized)));
    assert!(!state.media_relays.contains_room(&room_id));
}

#[tokio::test]
async fn revoked_user_cannot_reinsert_media_participant_or_track() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let survivor = state
        .registry
        .join(room_id.clone(), Default::default())
        .user;
    state.start_media_relay(room_id.clone(), StartMediaRelayRequest::default());
    state
        .media_relays
        .register_participant(room_id.clone(), survivor.id)
        .unwrap();
    let joined = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
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
    wait_for_access_revocation(&state, &room_id, &joined.user.id, &joined.access_token).await;

    let participant_state = state.clone();
    let participant_room_id = room_id.clone();
    let participant_user_id = joined.user.id.clone();
    let participant_token = joined.access_token.clone();
    let participant = tokio::spawn(async move {
        participant_state
            .register_authorized_media_participant(
                participant_room_id,
                participant_user_id,
                &participant_token,
            )
            .await
    });
    let track_state = state.clone();
    let track_room_id = room_id.clone();
    let track_user_id = joined.user.id.clone();
    let track_token = joined.access_token.clone();
    let track = tokio::spawn(async move {
        track_state
            .register_authorized_media_track(
                track_room_id,
                RegisterMediaTrackRequest {
                    user_id: track_user_id,
                    track_id: "audio-main".to_owned(),
                    kind: MediaTrackKind::Audio,
                },
                &track_token,
            )
            .await
    });
    drop(lifecycle_guard);

    assert!(leave.await.unwrap().unwrap().removed);
    assert!(matches!(
        participant.await.unwrap(),
        Err(ApiError::Unauthorized)
    ));
    assert!(matches!(track.await.unwrap(), Err(ApiError::Unauthorized)));
    assert!(state
        .media_relays
        .status(room_id)
        .participants
        .iter()
        .all(|participant| participant.user_id != joined.user.id));
}

#[tokio::test]
async fn revoked_user_cannot_apply_queued_media_control_mutations() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let survivor = state
        .registry
        .join(room_id.clone(), Default::default())
        .user;
    let joined = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap();
    state.start_media_relay(room_id.clone(), StartMediaRelayRequest::default());
    state
        .media_relays
        .register_track(
            room_id.clone(),
            RegisterMediaTrackRequest {
                user_id: survivor.id.clone(),
                track_id: "survivor-audio".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();
    state
        .media_relays
        .register_participant(room_id.clone(), joined.user.id.clone())
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
    wait_for_access_revocation(&state, &room_id, &joined.user.id, &joined.access_token).await;

    let subscriptions_state = state.clone();
    let subscriptions_room_id = room_id.clone();
    let subscriptions_user_id = joined.user.id.clone();
    let subscriptions_source_user_id = survivor.id;
    let subscriptions_token = joined.access_token.clone();
    let subscriptions = tokio::spawn(async move {
        subscriptions_state
            .update_authorized_media_relay_subscriptions(
                subscriptions_room_id,
                lyre_core::media::UpdateMediaRelaySubscriptionsRequest {
                    user_id: subscriptions_user_id,
                    source_user_ids: vec![subscriptions_source_user_id],
                },
                &subscriptions_token,
            )
            .await
    });
    let settings_state = state.clone();
    let settings_room_id = room_id.clone();
    let settings_user_id = joined.user.id.clone();
    let settings_token = joined.access_token.clone();
    let settings = tokio::spawn(async move {
        settings_state
            .update_authorized_media_relay_settings(
                settings_room_id,
                UpdateMediaRelaySettingsRequest {
                    user_id: settings_user_id,
                    noise: Default::default(),
                },
                &settings_token,
            )
            .await
    });
    let stop_state = state.clone();
    let stop_room_id = room_id.clone();
    let stop_user_id = joined.user.id.clone();
    let stop_token = joined.access_token.clone();
    let stop = tokio::spawn(async move {
        stop_state
            .stop_authorized_media_relay(
                stop_room_id,
                StopMediaRelayRequest {
                    user_id: stop_user_id,
                },
                &stop_token,
            )
            .await
    });
    let close_state = state.clone();
    let close_room_id = room_id.clone();
    let close_user_id = joined.user.id.clone();
    let close_token = joined.access_token.clone();
    let close = tokio::spawn(async move {
        close_state
            .close_authorized_server_media_session_for_user(
                close_room_id,
                close_user_id,
                &close_token,
            )
            .await
    });
    let candidate_state = state.clone();
    let candidate_room_id = room_id.clone();
    let candidate_user_id = joined.user.id.clone();
    let candidate_token = joined.access_token.clone();
    let candidate = tokio::spawn(async move {
        candidate_state
            .add_authorized_server_media_ice_candidate(
                ServerMediaIceCandidate {
                    room_id: candidate_room_id,
                    user_id: candidate_user_id,
                    candidate: "candidate:1 1 udp 1 127.0.0.1 9 typ host".to_owned(),
                    sdp_mid: Some("0".to_owned()),
                    sdp_mline_index: Some(0),
                    username_fragment: None,
                },
                &candidate_token,
            )
            .await
    });
    drop(lifecycle_guard);

    assert!(leave.await.unwrap().unwrap().removed);
    assert!(matches!(
        subscriptions.await.unwrap(),
        Err(ApiError::Unauthorized)
    ));
    assert!(matches!(
        settings.await.unwrap(),
        Err(ApiError::Unauthorized)
    ));
    assert!(matches!(stop.await.unwrap(), Err(ApiError::Unauthorized)));
    assert!(matches!(close.await.unwrap(), Err(ApiError::Unauthorized)));
    assert!(matches!(
        candidate.await.unwrap(),
        Err(ApiError::Unauthorized)
    ));
    assert_eq!(
        state.media_relays.status(room_id).status,
        MediaRelayStatus::Active
    );
}
