use crate::{api::AppState, state_persistence::RoomStatePersistence};
use lyre_core::{
    MediaTrackKind, PersistedRoom, PersistedRoomRegistry, PersistedRoomUser,
    PersistedWebSocketAdmission, RegisterMediaTrackRequest, RoomAccessToken, RoomId,
    StartMediaRelayRequest, UserId, UserProfile,
};
use lyre_noise_cancelling::DeepFilterNetRuntimeConfig;
use lyre_webrtc::{ServerMediaOffer, WebRtcStack};
use std::{
    convert::Infallible,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
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
        websocket_admission: PersistedWebSocketAdmission::Confirmed,
    }
}

async fn offer_sdp() -> String {
    let offerer = WebRtcStack::new().create_peer_connection().await.unwrap();
    offerer.create_local_offer_for_test().await.unwrap()
}

#[tokio::test]
async fn websocket_close_frame_runs_cleanup_without_peer_traffic() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let leaving = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap();
    let staying = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap()
        .user;
    let (staying_tx, mut staying_rx) =
        tokio::sync::mpsc::channel(crate::signalling::SIGNAL_CHANNEL_CAPACITY);
    state.peers.connect(
        &state.registry,
        room_id.clone(),
        staying.id.clone(),
        staying_tx,
    );

    tokio::time::timeout(
        Duration::from_secs(1),
        crate::api::handle_socket_io(
            state.clone(),
            room_id.clone(),
            leaving.user.id.clone(),
            futures_util::sink::drain(),
            futures_util::stream::iter([Ok::<_, Infallible>(axum::extract::ws::Message::Close(
                None,
            ))]),
        ),
    )
    .await
    .expect("websocket close should finish the handler");

    assert!(state
        .registry
        .validate_access_token(&room_id, &leaving.user.id, &leaving.access_token)
        .is_err());
    let snapshot = state.registry.snapshot(room_id.clone());
    assert_eq!(snapshot.users, vec![staying]);
    assert!(matches!(
        staying_rx.recv().await.unwrap().payload,
        crate::signalling::SignalPayload::UserLeft { user_id }
            if user_id == leaving.user.id
    ));
    assert_eq!(crate::metrics::snapshot(&state).leaves, 1);
}

#[tokio::test]
async fn websocket_eof_runs_cleanup_without_peer_traffic() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let leaving = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap();

    tokio::time::timeout(
        Duration::from_secs(1),
        crate::api::handle_socket_io(
            state.clone(),
            room_id.clone(),
            leaving.user.id.clone(),
            futures_util::sink::drain(),
            futures_util::stream::empty::<Result<axum::extract::ws::Message, Infallible>>(),
        ),
    )
    .await
    .expect("websocket EOF should finish the handler");

    assert!(state
        .registry
        .validate_access_token(&room_id, &leaving.user.id, &leaving.access_token)
        .is_err());
    assert!(state.registry.snapshot(room_id).users.is_empty());
    assert_eq!(crate::metrics::snapshot(&state).leaves, 1);
}

#[tokio::test]
async fn replaced_socket_teardown_keeps_new_socket_and_membership() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let joined = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap();
    let (snapshot_sent_tx, snapshot_sent_rx) = tokio::sync::oneshot::channel();
    let sink = Box::pin(futures_util::sink::unfold(
        Some(snapshot_sent_tx),
        |mut snapshot_sent, _message| async move {
            if let Some(snapshot_sent) = snapshot_sent.take() {
                let _ = snapshot_sent.send(());
            }
            Ok::<_, Infallible>(snapshot_sent)
        },
    ));
    let stream = futures_util::stream::pending::<Result<axum::extract::ws::Message, Infallible>>();
    let handler = tokio::spawn(crate::api::handle_socket_io(
        state.clone(),
        room_id.clone(),
        joined.user.id.clone(),
        sink,
        stream,
    ));
    snapshot_sent_rx.await.unwrap();

    let (replacement_tx, mut replacement_rx) =
        tokio::sync::mpsc::channel(crate::signalling::SIGNAL_CHANNEL_CAPACITY);
    state.peers.connect(
        &state.registry,
        room_id.clone(),
        joined.user.id.clone(),
        replacement_tx,
    );
    tokio::time::timeout(Duration::from_secs(1), handler)
        .await
        .expect("replacing a websocket should revoke and finish the old handler")
        .unwrap();

    assert!(state
        .registry
        .validate_access_token(&room_id, &joined.user.id, &joined.access_token)
        .is_ok());
    assert_eq!(state.registry.snapshot(room_id.clone()).users.len(), 1);
    assert_eq!(crate::metrics::snapshot(&state).leaves, 0);
    assert_eq!(
        state
            .peers
            .forward(crate::signalling::SignalMessage::new(
                room_id,
                UserId::from_external("peer"),
                Some(joined.user.id),
                crate::signalling::SignalPayload::Offer { sdp: "sdp".into() },
            ))
            .delivered,
        1
    );
    replacement_rx.recv().await.unwrap();
}

#[tokio::test]
async fn removing_peer_only_does_not_change_room_membership() {
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
    let (leaving_tx, _leaving_rx) =
        tokio::sync::mpsc::channel(crate::signalling::SIGNAL_CHANNEL_CAPACITY);
    let (staying_tx, mut staying_rx) =
        tokio::sync::mpsc::channel(crate::signalling::SIGNAL_CHANNEL_CAPACITY);
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

    state.peers.remove_peer(&room_id, &leaving.id);

    let snapshot = state.registry.snapshot(room_id.clone());
    assert_eq!(snapshot.users.len(), 2);
    assert!(snapshot.users.iter().any(|user| user.id == leaving.id));
    assert!(snapshot.users.iter().any(|user| user.id == staying.id));
    assert!(staying_rx.try_recv().is_err());
    let metrics = crate::metrics::snapshot(&state);
    assert_eq!(metrics.leaves, 0);
}

#[tokio::test]
async fn removing_peer_after_persisted_leave_does_not_duplicate_leave() {
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
    let (leaving_tx, _leaving_rx) =
        tokio::sync::mpsc::channel(crate::signalling::SIGNAL_CHANNEL_CAPACITY);
    let (staying_tx, mut staying_rx) =
        tokio::sync::mpsc::channel(crate::signalling::SIGNAL_CHANNEL_CAPACITY);
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
    state.peers.remove_peer(&room_id, &leaving.id);

    assert!(staying_rx.try_recv().is_err());
    let metrics = crate::metrics::snapshot(&state);
    assert_eq!(metrics.leaves, 1);
}

#[tokio::test]
async fn websocket_disconnect_closes_departed_user_server_media_state() {
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
    state
        .media_relays
        .start(room_id.clone(), StartMediaRelayRequest::default());
    for user_id in [&leaving.id, &staying.id] {
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
    }
    state
        .answer_server_media_offer(ServerMediaOffer {
            room_id: room_id.clone(),
            user_id: leaving.id.clone(),
            audio_track_id: "audio-main".to_owned(),
            sdp: offer_sdp().await,
        })
        .await
        .unwrap();

    state.disconnect_room_socket(&room_id, &leaving.id).await;

    let relay = state.media_relays.status(room_id);
    assert_eq!(relay.participants.len(), 1);
    assert_eq!(relay.participants[0].user_id, staying.id);
    assert!(state.active_server_media_sessions().is_empty());
    assert_eq!(state.server_media_runtime_pump_count(), 0);
    assert_eq!(state.server_media_peer_connection_count(), 0);
    assert_eq!(crate::metrics::snapshot(&state).leaves, 1);
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
    let (leaving_tx, _leaving_rx) =
        tokio::sync::mpsc::channel(crate::signalling::SIGNAL_CHANNEL_CAPACITY);
    let (peer_tx, mut peer_rx) =
        tokio::sync::mpsc::channel(crate::signalling::SIGNAL_CHANNEL_CAPACITY);
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
