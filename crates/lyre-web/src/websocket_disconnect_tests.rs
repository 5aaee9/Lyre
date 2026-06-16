use crate::{api::AppState, state_persistence::RoomStatePersistence};
use lyre_core::{
    PersistedRoom, PersistedRoomRegistry, PersistedRoomUser, RoomAccessToken, RoomId, UserId,
    UserProfile,
};
use lyre_noise_cancelling::DeepFilterNetRuntimeConfig;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_state_path(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "lyre-ws-disconnect-{name}-{}-{}.json",
        std::process::id(),
        TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_file(&path);
    path
}

fn persisted_user(user_id: &str, token: &str) -> PersistedRoomUser {
    PersistedRoomUser {
        profile: UserProfile {
            id: UserId::from_external(user_id),
            nickname: "Ada".to_owned(),
            joined_at: chrono::Utc::now(),
            noise: Default::default(),
        },
        access_token: RoomAccessToken::from_external(token),
    }
}

#[tokio::test]
async fn websocket_disconnect_removes_room_user_and_broadcasts_user_left() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let leaving = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap()
        .user;
    let staying = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap()
        .user;
    let (leaving_tx, _leaving_rx) = tokio::sync::mpsc::unbounded_channel();
    let (staying_tx, mut staying_rx) = tokio::sync::mpsc::unbounded_channel();
    state.peers.connect(
        &state.registry,
        room_id.clone(),
        leaving.id.clone(),
        leaving_tx,
    );
    state.peers.connect(
        &state.registry,
        room_id.clone(),
        staying.id.clone(),
        staying_tx,
    );

    state.disconnect_room_socket(&room_id, &leaving.id).await;

    let snapshot = state.registry.snapshot(room_id.clone());
    assert_eq!(snapshot.users.len(), 1);
    assert_eq!(snapshot.users[0].id, staying.id);
    let signal = staying_rx.try_recv().unwrap();
    assert_eq!(
        signal.payload,
        crate::signalling::SignalPayload::UserLeft {
            user_id: leaving.id
        }
    );
    let metrics = crate::metrics::snapshot(&state);
    assert_eq!(metrics.leaves, 1);
}

#[tokio::test]
async fn websocket_disconnect_after_rest_leave_only_removes_socket() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let leaving = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap()
        .user;
    let staying = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap()
        .user;
    let (leaving_tx, _leaving_rx) = tokio::sync::mpsc::unbounded_channel();
    let (staying_tx, mut staying_rx) = tokio::sync::mpsc::unbounded_channel();
    state.peers.connect(
        &state.registry,
        room_id.clone(),
        leaving.id.clone(),
        leaving_tx,
    );
    state.peers.connect(
        &state.registry,
        room_id.clone(),
        staying.id.clone(),
        staying_tx,
    );

    let response = state
        .leave_room_persisted(&room_id, &leaving.id)
        .await
        .unwrap();
    assert!(response.removed);
    state.disconnect_room_socket(&room_id, &leaving.id).await;

    assert!(staying_rx.try_recv().is_err());
    let metrics = crate::metrics::snapshot(&state);
    assert_eq!(metrics.leaves, 1);
}

#[tokio::test]
async fn websocket_disconnect_updates_persisted_room_state() {
    let path = unique_state_path("persisted");
    std::fs::write(
        &path,
        serde_json::to_vec(&PersistedRoomRegistry {
            rooms: vec![PersistedRoom {
                room_id: RoomId::default_room(),
                users: vec![persisted_user("user_a", "token_a")],
            }],
        })
        .unwrap(),
    )
    .unwrap();
    let state = AppState::with_room_state_persistence(
        Default::default(),
        None,
        Some(RoomStatePersistence::new(path.clone())),
        DeepFilterNetRuntimeConfig::default(),
    )
    .unwrap();

    state
        .disconnect_room_socket(&RoomId::default_room(), &UserId::from_external("user_a"))
        .await;

    let file = std::fs::read_to_string(&path).unwrap();
    assert!(!file.contains("user_a"));
    assert!(!file.contains("token_a"));
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn websocket_disconnect_persistence_failure_rolls_back_without_broadcast() {
    let path = unique_state_path("rollback");
    std::fs::write(
        &path,
        serde_json::to_vec(&PersistedRoomRegistry {
            rooms: vec![PersistedRoom {
                room_id: RoomId::default_room(),
                users: vec![persisted_user("user_a", "token_a")],
            }],
        })
        .unwrap(),
    )
    .unwrap();
    let bad_path = unique_state_path("bad-rollback");
    let state = AppState::with_room_state_persistence(
        Default::default(),
        None,
        Some(RoomStatePersistence::new(path.clone())),
        DeepFilterNetRuntimeConfig::default(),
    )
    .unwrap();
    state
        .set_room_state_persistence_for_tests(Some(RoomStatePersistence::always_fail_for_tests(
            bad_path.clone(),
        )))
        .await;
    let room_id = RoomId::default_room();
    let (leaving_tx, _leaving_rx) = tokio::sync::mpsc::unbounded_channel();
    let (peer_tx, mut peer_rx) = tokio::sync::mpsc::unbounded_channel();
    state.peers.connect(
        &state.registry,
        room_id.clone(),
        UserId::from_external("user_a"),
        leaving_tx,
    );
    state.peers.connect(
        &state.registry,
        room_id.clone(),
        UserId::from_external("peer"),
        peer_tx,
    );

    state
        .disconnect_room_socket(&room_id, &UserId::from_external("user_a"))
        .await;

    assert!(state
        .registry
        .validate_access_token(
            &room_id,
            &UserId::from_external("user_a"),
            &RoomAccessToken::from_external("token_a"),
        )
        .is_ok());
    assert!(peer_rx.try_recv().is_err());
    let delivered = state.peers.forward(crate::signalling::SignalMessage::new(
        room_id,
        UserId::from_external("peer"),
        Some(UserId::from_external("user_a")),
        crate::signalling::SignalPayload::Offer { sdp: "sdp".into() },
    ));
    assert_eq!(delivered.delivered, 0);
    let metrics = crate::metrics::snapshot(&state);
    assert_eq!(metrics.leaves, 0);
    assert_eq!(metrics.persistence_failures, 1);
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(bad_path);
}
