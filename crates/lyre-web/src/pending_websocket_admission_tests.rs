use crate::state_persistence::RoomStatePersistence;
use crate::{api::AppState, error::ApiError, signalling::SignalPayload};
use axum::{http::StatusCode, response::IntoResponse};
use lyre_core::{
    PersistedRoom, PersistedRoomRegistry, PersistedRoomUser, PersistedWebSocketAdmission,
    RoomAccessToken, RoomId, UserId, UserProfile,
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
        "lyre-pending-websocket-{name}-{}-{}.json",
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
        websocket_admission: PersistedWebSocketAdmission::Confirmed,
    }
}

#[tokio::test]
async fn join_without_websocket_is_removed_when_admission_lease_expires() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let staying = state
        .registry
        .join(room_id.clone(), Default::default())
        .user;
    let (staying_tx, mut staying_rx) =
        tokio::sync::mpsc::channel(crate::signalling::SIGNAL_CHANNEL_CAPACITY);
    state.peers.connect(
        &state.registry,
        room_id.clone(),
        staying.id.clone(),
        staying_tx,
    );
    let expiring = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap();

    let expired = state
        .expire_pending_websocket_admission_for_tests(&room_id, &expiring.user.id)
        .await
        .unwrap();

    assert!(expired);
    assert!(state
        .registry
        .validate_access_token(&room_id, &expiring.user.id, &expiring.access_token)
        .is_err());
    assert_eq!(state.registry.snapshot(room_id).users, vec![staying]);
    assert!(matches!(
        staying_rx.recv().await.unwrap().payload,
        SignalPayload::UserLeft { user_id } if user_id == expiring.user.id
    ));
}

#[tokio::test]
async fn pending_admission_limit_rejects_join_without_growing_registry() {
    let state = AppState::default();
    state.set_pending_websocket_admission_limit_for_tests(1);
    let room_id = RoomId::default_room();
    let first = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap();

    let error = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap_err();

    assert!(matches!(error, ApiError::PendingWebSocketAdmissionsFull));
    assert_eq!(state.pending_websocket_admission_count_for_tests(), 1);
    assert_eq!(state.registry.snapshot(room_id.clone()).users.len(), 1);

    let (peer_tx, _peer_rx) =
        tokio::sync::mpsc::channel(crate::signalling::SIGNAL_CHANNEL_CAPACITY);
    state
        .connect_room_socket(room_id.clone(), first.user.id, &first.access_token, peer_tx)
        .await
        .unwrap();
    assert_eq!(state.pending_websocket_admission_count_for_tests(), 0);

    state
        .join_room_persisted(room_id, Default::default())
        .await
        .unwrap();
    assert_eq!(state.pending_websocket_admission_count_for_tests(), 1);
}

#[tokio::test]
async fn websocket_admission_queued_before_expiration_keeps_membership() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let joined = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap();
    let lifecycle_guard = state.room_state_persistence_lock.lock().await;

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

    let (expiration_started_tx, expiration_started_rx) = tokio::sync::oneshot::channel();
    let expiration_state = state.clone();
    let expiration_room_id = room_id.clone();
    let expiration_user_id = joined.user.id.clone();
    let expiration = tokio::spawn(async move {
        expiration_started_tx.send(()).unwrap();
        expiration_state
            .expire_pending_websocket_admission_for_tests(&expiration_room_id, &expiration_user_id)
            .await
    });
    expiration_started_rx.await.unwrap();
    tokio::task::yield_now().await;
    drop(lifecycle_guard);

    admission.await.unwrap().unwrap();
    assert!(!expiration.await.unwrap().unwrap());
    assert!(state
        .registry
        .validate_access_token(&room_id, &joined.user.id, &joined.access_token)
        .is_ok());
    assert_eq!(state.pending_websocket_admission_count_for_tests(), 0);
}

#[tokio::test]
async fn failed_expiration_persistence_keeps_lease_for_retry() {
    let path = unique_state_path("retry");
    let state = AppState::with_room_state_persistence(
        Default::default(),
        None,
        Some(RoomStatePersistence::new(path.clone())),
        DeepFilterNetRuntimeConfig::default(),
    )
    .unwrap();
    let room_id = RoomId::default_room();
    let joined = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap();
    state
        .set_room_state_persistence_for_tests(Some(RoomStatePersistence::always_fail_for_tests(
            path.clone(),
        )))
        .await;

    let error = state
        .expire_pending_websocket_admission_for_tests(&room_id, &joined.user.id)
        .await
        .unwrap_err();

    let ApiError::Persistence(error) = error else {
        panic!("expiration should return the persistence error");
    };
    assert!(format!("{error:#}").contains("forced Lyre room state write failure"));
    assert_eq!(state.pending_websocket_admission_count_for_tests(), 1);
    assert!(state
        .registry
        .validate_access_token(&room_id, &joined.user.id, &joined.access_token)
        .is_ok());

    state
        .set_room_state_persistence_for_tests(Some(RoomStatePersistence::new(path.clone())))
        .await;
    assert!(state
        .expire_pending_websocket_admission_for_tests(&room_id, &joined.user.id)
        .await
        .unwrap());
    assert_eq!(state.pending_websocket_admission_count_for_tests(), 0);
    assert!(state.registry.snapshot(room_id).users.is_empty());
    assert!(!std::fs::read_to_string(&path)
        .unwrap()
        .contains(joined.user.id.as_str()));
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn restored_confirmed_users_are_not_treated_as_pending_admissions() {
    let path = unique_state_path("restored");
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
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user_a");

    assert_eq!(state.pending_websocket_admission_count_for_tests(), 0);
    assert!(!state
        .expire_pending_websocket_admission_for_tests(&room_id, &user_id)
        .await
        .unwrap());
    assert!(state
        .registry
        .validate_access_token(
            &room_id,
            &user_id,
            &RoomAccessToken::from_external("token_a"),
        )
        .is_ok());
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn pending_admission_is_restored_after_restart_and_can_expire() {
    let path = unique_state_path("pending-restart");
    let persistence = RoomStatePersistence::new(path.clone());
    let state = AppState::with_room_state_persistence(
        Default::default(),
        None,
        Some(persistence.clone()),
        DeepFilterNetRuntimeConfig::default(),
    )
    .unwrap();
    let room_id = RoomId::default_room();
    let joined = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap();

    let restored = AppState::with_room_state_persistence(
        Default::default(),
        None,
        Some(persistence),
        DeepFilterNetRuntimeConfig::default(),
    )
    .unwrap();

    assert_eq!(restored.pending_websocket_admission_count_for_tests(), 1);
    assert!(restored
        .expire_pending_websocket_admission_for_tests(&room_id, &joined.user.id)
        .await
        .unwrap());
    assert!(restored.registry.snapshot(room_id).users.is_empty());
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn confirmed_admission_is_persisted_before_restart() {
    let path = unique_state_path("confirmed-restart");
    let persistence = RoomStatePersistence::new(path.clone());
    let state = AppState::with_room_state_persistence(
        Default::default(),
        None,
        Some(persistence.clone()),
        DeepFilterNetRuntimeConfig::default(),
    )
    .unwrap();
    let room_id = RoomId::default_room();
    let joined = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap();
    let (peer_tx, _peer_rx) =
        tokio::sync::mpsc::channel(crate::signalling::SIGNAL_CHANNEL_CAPACITY);

    state
        .connect_room_socket(
            room_id.clone(),
            joined.user.id.clone(),
            &joined.access_token,
            peer_tx,
        )
        .await
        .unwrap();
    let restored = AppState::with_room_state_persistence(
        Default::default(),
        None,
        Some(persistence),
        DeepFilterNetRuntimeConfig::default(),
    )
    .unwrap();

    assert_eq!(restored.pending_websocket_admission_count_for_tests(), 0);
    assert!(restored
        .registry
        .validate_access_token(&room_id, &joined.user.id, &joined.access_token)
        .is_ok());
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn expired_persisted_admission_is_pruned_before_startup_completes() {
    let path = unique_state_path("expired-restart");
    let mut user = persisted_user("user_a", "token_a");
    user.websocket_admission = PersistedWebSocketAdmission::Pending {
        expires_at: chrono::Utc::now() - chrono::Duration::seconds(1),
    };
    std::fs::write(
        &path,
        serde_json::to_vec(&PersistedRoomRegistry {
            rooms: vec![PersistedRoom {
                room_id: RoomId::default_room(),
                users: vec![user],
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

    assert_eq!(state.pending_websocket_admission_count_for_tests(), 0);
    assert!(state
        .registry
        .snapshot(RoomId::default_room())
        .users
        .is_empty());
    assert!(!std::fs::read_to_string(&path).unwrap().contains("user_a"));
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn restored_pending_admissions_count_toward_the_global_limit() {
    let path = unique_state_path("restored-cap");
    let mut user = persisted_user("user_a", "token_a");
    user.websocket_admission = PersistedWebSocketAdmission::Pending {
        expires_at: chrono::Utc::now() + chrono::Duration::minutes(1),
    };
    std::fs::write(
        &path,
        serde_json::to_vec(&PersistedRoomRegistry {
            rooms: vec![PersistedRoom {
                room_id: RoomId::default_room(),
                users: vec![user],
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
    state.set_pending_websocket_admission_limit_for_tests(1);

    let error = state
        .join_room_persisted(RoomId::default_room(), Default::default())
        .await
        .unwrap_err();

    assert!(matches!(error, ApiError::PendingWebSocketAdmissionsFull));
    assert_eq!(state.registry.aggregate().users, 1);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn missing_websocket_admission_field_is_a_startup_error() {
    let path = unique_state_path("missing-admission-field");
    let mut persisted = serde_json::to_value(PersistedRoomRegistry {
        rooms: vec![PersistedRoom {
            room_id: RoomId::default_room(),
            users: vec![persisted_user("user_a", "token_a")],
        }],
    })
    .unwrap();
    persisted["rooms"][0]["users"][0]
        .as_object_mut()
        .unwrap()
        .remove("websocket_admission");
    std::fs::write(&path, serde_json::to_vec(&persisted).unwrap()).unwrap();

    let error = AppState::with_room_state_persistence(
        Default::default(),
        None,
        Some(RoomStatePersistence::new(path.clone())),
        DeepFilterNetRuntimeConfig::default(),
    )
    .unwrap_err();

    assert!(format!("{error:#}").contains("failed to parse Lyre room state"));
    assert!(format!("{error:#}").contains("websocket_admission"));
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn explicit_leave_cancels_pending_admission_lease() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let joined = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap();
    assert_eq!(state.pending_websocket_admission_count_for_tests(), 1);

    let response = state
        .leave_room_persisted(&room_id, &joined.user.id)
        .await
        .unwrap();

    assert!(response.removed);
    assert_eq!(state.pending_websocket_admission_count_for_tests(), 0);
    assert!(!state
        .expire_pending_websocket_admission_for_tests(&room_id, &joined.user.id)
        .await
        .unwrap());
}

#[test]
fn pending_admission_limit_error_maps_to_too_many_requests() {
    assert_eq!(
        ApiError::PendingWebSocketAdmissionsFull
            .into_response()
            .status(),
        StatusCode::TOO_MANY_REQUESTS
    );
}
