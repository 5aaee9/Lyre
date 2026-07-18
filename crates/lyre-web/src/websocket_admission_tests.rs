use crate::{api::AppState, signalling::SignalMessage};
use lyre_core::{RoomId, UserId};

#[tokio::test]
async fn leave_queued_before_websocket_admission_prevents_ghost_peer() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let joined = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap();
    let lifecycle_guard = state.room_state_persistence_lock.lock().await;

    let (leave_started_tx, leave_started_rx) = tokio::sync::oneshot::channel();
    let leave_state = state.clone();
    let leave_room_id = room_id.clone();
    let leave_user_id = joined.user.id.clone();
    let leave = tokio::spawn(async move {
        leave_started_tx.send(()).unwrap();
        leave_state
            .leave_room_persisted(&leave_room_id, &leave_user_id)
            .await
    });
    leave_started_rx.await.unwrap();
    tokio::task::yield_now().await;

    let (admission_started_tx, admission_started_rx) = tokio::sync::oneshot::channel();
    let admission_state = state.clone();
    let admission_room_id = room_id.clone();
    let admission_user_id = joined.user.id.clone();
    let admission_token = joined.access_token.clone();
    let (peer_tx, _peer_rx) =
        tokio::sync::mpsc::channel(crate::signalling::SIGNAL_CHANNEL_CAPACITY);
    let admission = tokio::spawn(async move {
        admission_started_tx.send(()).unwrap();
        admission_state
            .connect_room_socket(
                admission_room_id,
                admission_user_id,
                &admission_token,
                peer_tx,
            )
            .await
    });
    admission_started_rx.await.unwrap();
    tokio::task::yield_now().await;
    drop(lifecycle_guard);

    assert!(leave.await.unwrap().unwrap().removed);
    assert!(admission.await.unwrap().is_err());
    assert_eq!(
        state
            .peers
            .forward(SignalMessage::error(
                room_id,
                UserId::from_external("peer"),
                "test",
            ))
            .delivered,
        0
    );
}

#[tokio::test]
async fn replacement_admitted_before_old_disconnect_keeps_new_peer_and_membership() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let joined = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap();
    let (old_tx, _old_rx) = tokio::sync::mpsc::channel(crate::signalling::SIGNAL_CHANNEL_CAPACITY);
    let old_identity = old_tx.clone();
    state.peers.connect(
        &state.registry,
        room_id.clone(),
        joined.user.id.clone(),
        old_tx,
    );
    let lifecycle_guard = state.room_state_persistence_lock.lock().await;

    let (admission_started_tx, admission_started_rx) = tokio::sync::oneshot::channel();
    let admission_state = state.clone();
    let admission_room_id = room_id.clone();
    let admission_user_id = joined.user.id.clone();
    let admission_token = joined.access_token.clone();
    let (new_tx, mut new_rx) =
        tokio::sync::mpsc::channel(crate::signalling::SIGNAL_CHANNEL_CAPACITY);
    let admission = tokio::spawn(async move {
        admission_started_tx.send(()).unwrap();
        admission_state
            .connect_room_socket(
                admission_room_id,
                admission_user_id,
                &admission_token,
                new_tx,
            )
            .await
    });
    admission_started_rx.await.unwrap();
    tokio::task::yield_now().await;

    let disconnect_state = state.clone();
    let disconnect_room_id = room_id.clone();
    let disconnect_user_id = joined.user.id.clone();
    let disconnect = tokio::spawn(async move {
        disconnect_state
            .disconnect_room_socket_if_current(
                &disconnect_room_id,
                &disconnect_user_id,
                &old_identity,
            )
            .await
    });
    tokio::task::yield_now().await;
    drop(lifecycle_guard);

    admission.await.unwrap().unwrap();
    assert!(!disconnect.await.unwrap());
    assert!(state
        .registry
        .validate_access_token(&room_id, &joined.user.id, &joined.access_token)
        .is_ok());
    assert_eq!(
        state
            .peers
            .forward(SignalMessage::error(
                room_id,
                UserId::from_external("peer"),
                "test",
            ))
            .delivered,
        1
    );
    new_rx.recv().await.unwrap();
}
