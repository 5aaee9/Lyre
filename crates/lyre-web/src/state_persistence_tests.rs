use crate::{
    api::{router, AppState},
    state_persistence::RoomStatePersistence,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use lyre_core::{
    PersistedRoom, PersistedRoomRegistry, PersistedRoomUser, RoomAccessToken, RoomId, UserId,
    UserProfile,
};
use lyre_noise_cancelling::DeepFilterNetRuntimeConfig;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use tower::ServiceExt;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_state_path(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "lyre-api-{name}-{}-{}.json",
        std::process::id(),
        TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_file(&path);
    path
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
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
async fn state_file_load_makes_persisted_users_visible() {
    let path = unique_state_path("load");
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
    let app = router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/rooms/DEFAULT")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = body_json(response).await;
    assert_eq!(body["users"][0]["id"], "user_a");
    assert!(!body.to_string().contains("token_a"));
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn restored_token_authorizes_leave_and_rewrites_state_file() {
    let path = unique_state_path("leave");
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
    let app = router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rooms/DEFAULT/leave")
                .header("content-type", "application/json")
                .header("authorization", "Bearer token_a")
                .body(Body::from(r#"{"user_id":"user_a"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let file = std::fs::read_to_string(&path).unwrap();
    assert!(!file.contains("user_a"));
    assert!(!file.contains("token_a"));
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn successful_join_writes_user_and_token_to_state_file() {
    let path = unique_state_path("join");
    let state = AppState::with_room_state_persistence(
        Default::default(),
        None,
        Some(RoomStatePersistence::new(path.clone())),
        DeepFilterNetRuntimeConfig::default(),
    )
    .unwrap();
    let app = router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rooms/DEFAULT/join")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"nickname":"Ada"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = body_json(response).await;
    let file = std::fs::read_to_string(&path).unwrap();
    assert!(file.contains(body["user"]["id"].as_str().unwrap()));
    assert!(file.contains(body["access_token"].as_str().unwrap()));
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn failed_persisted_join_rolls_back_user_without_token_response() {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "lyre-missing-parent-{}-{}",
        std::process::id(),
        TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    path.push("state.json");
    let path_text = path.display().to_string();
    let state = AppState::with_room_state_persistence(
        Default::default(),
        None,
        Some(RoomStatePersistence::new(path.clone())),
        DeepFilterNetRuntimeConfig::default(),
    )
    .unwrap_err();

    assert!(
        format!("{state:#}").contains("Lyre room state parent directory does not exist"),
        "{state:#}"
    );

    let state = AppState::with_room_state_persistence(
        Default::default(),
        None,
        None,
        DeepFilterNetRuntimeConfig::default(),
    )
    .unwrap();
    state
        .set_room_state_persistence_for_tests(Some(RoomStatePersistence::new(path.clone())))
        .await;
    let app = router(state.clone());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rooms/DEFAULT/join")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"nickname":"Ada"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = body_json(response).await;
    assert_eq!(body["error"], "room state persistence failed");
    assert!(!body.to_string().contains("access_token"));
    assert!(!body.to_string().contains(&path_text));
    assert!(!body.to_string().contains(".tmp"));
    assert!(!body.to_string().contains("No such file"));
    assert!(!body.to_string().contains("failed to write"));
    assert!(state
        .registry
        .snapshot(RoomId::default_room())
        .users
        .is_empty());
    let metrics = crate::metrics::snapshot(&state);
    assert_eq!(metrics.joins, 0);
    assert_eq!(metrics.persistence_failures, 1);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn failed_persisted_leave_rolls_back_user_and_token() {
    let path = unique_state_path("leave-rollback");
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
    let bad_path = unique_state_path("bad-leave-rollback");
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
    let app = router(state.clone());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rooms/DEFAULT/leave")
                .header("content-type", "application/json")
                .header("authorization", "Bearer token_a")
                .body(Body::from(r#"{"user_id":"user_a"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = body_json(response).await;
    assert_eq!(body["error"], "room state persistence failed");
    assert!(!body
        .to_string()
        .contains("forced Lyre room state write failure"));
    assert!(state
        .registry
        .validate_access_token(
            &RoomId::default_room(),
            &UserId::from_external("user_a"),
            &RoomAccessToken::from_external("token_a"),
        )
        .is_ok());
    let metrics = crate::metrics::snapshot(&state);
    assert_eq!(metrics.leaves, 0);
    assert_eq!(metrics.persistence_failures, 1);
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(bad_path);
}

#[test]
fn malformed_state_file_fails_state_construction_with_context() {
    let path = unique_state_path("malformed-api");
    std::fs::write(&path, "{not json").unwrap();

    let error = AppState::with_room_state_persistence(
        Default::default(),
        None,
        Some(RoomStatePersistence::new(path.clone())),
        DeepFilterNetRuntimeConfig::default(),
    )
    .unwrap_err();

    assert!(format!("{error:#}").contains("failed to parse Lyre room state"));
    let _ = std::fs::remove_file(path);
}

#[test]
fn missing_parent_state_file_fails_state_construction_with_context() {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "lyre-missing-startup-parent-{}-{}",
        std::process::id(),
        TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    path.push("state.json");

    let error = AppState::with_room_state_persistence(
        Default::default(),
        None,
        Some(RoomStatePersistence::new(path)),
        DeepFilterNetRuntimeConfig::default(),
    )
    .unwrap_err();

    assert!(
        format!("{error:#}").contains("Lyre room state parent directory does not exist"),
        "{error:#}"
    );
}

#[test]
fn invalid_deepfilternet_runtime_fails_state_construction_with_context() {
    let error = AppState::with_room_state_persistence(
        Default::default(),
        None,
        None,
        DeepFilterNetRuntimeConfig {
            fft_size: 480,
            hop_size: 480,
            ..DeepFilterNetRuntimeConfig::default()
        },
    )
    .unwrap_err();

    assert!(format!("{error:#}").contains("invalid DeepFilterNet runtime config"));
    assert!(format!("{error:#}").contains("hop_size * 2 must be <= fft_size"));
}
