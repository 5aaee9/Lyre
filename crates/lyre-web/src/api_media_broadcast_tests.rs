use crate::api::{router, AppState};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use lyre_core::{
    AudioFrame, MediaTrackKind, NoiseCancellationConfig, NoiseProvider, RegisterMediaTrackRequest,
    RoomId, StartMediaRelayRequest, UserId,
};
use tokio::time::{timeout, Duration};
use tower::ServiceExt;

fn post_json_with_auth(uri: &str, body: String, access_token: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access_token}"))
        .body(Body::from(body))
        .unwrap()
}

fn audio_frame(room_id: RoomId, user_id: UserId, samples: Vec<f32>) -> AudioFrame {
    AudioFrame {
        room_id,
        user_id,
        track_id: "audio-main".to_owned(),
        sample_rate_hz: 48_000,
        channels: 1,
        sequence: 1,
        samples,
    }
}

fn start_relay_with_track(
    state: &AppState,
    room_id: RoomId,
    user_id: UserId,
    provider: NoiseProvider,
) {
    state.media_relays.start(
        room_id.clone(),
        StartMediaRelayRequest {
            noise: Some(NoiseCancellationConfig {
                provider,
                intensity: 0.5,
                voice_activity_threshold: 0.35,
            }),
        },
    );
    state
        .media_relays
        .register_track(
            room_id,
            RegisterMediaTrackRequest {
                user_id,
                track_id: "audio-main".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();
}

fn process_samples(state: &AppState, room_id: &RoomId, user_id: &UserId, samples: Vec<f32>) {
    state
        .process_media_frame(audio_frame(room_id.clone(), user_id.clone(), samples))
        .unwrap();
}

#[tokio::test]
async fn processed_media_subscriber_receives_future_frames() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user_01");
    start_relay_with_track(&state, room_id.clone(), user_id.clone(), NoiseProvider::Off);
    let mut frames = state.subscribe_processed_media_frames(&room_id);

    process_samples(&state, &room_id, &user_id, vec![0.25, -0.5, 0.75]);

    let frame = timeout(Duration::from_millis(100), frames.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(frame.samples, vec![0.25, -0.5, 0.75]);
    assert_eq!(state.processed_media_frames(&room_id), vec![frame]);
}

#[tokio::test]
async fn processed_media_subscribers_are_room_scoped() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let other_room_id = RoomId::parse_boundary("OTHER").unwrap();
    let user_id = UserId::from_external("user_01");
    start_relay_with_track(&state, room_id.clone(), user_id.clone(), NoiseProvider::Off);
    start_relay_with_track(
        &state,
        other_room_id.clone(),
        user_id.clone(),
        NoiseProvider::Off,
    );
    let mut frames = state.subscribe_processed_media_frames(&room_id);
    let mut other_frames = state.subscribe_processed_media_frames(&other_room_id);

    process_samples(&state, &other_room_id, &user_id, vec![1.0]);
    assert!(timeout(Duration::from_millis(25), frames.recv())
        .await
        .is_err());

    process_samples(&state, &room_id, &user_id, vec![2.0]);

    assert_eq!(
        timeout(Duration::from_millis(100), frames.recv())
            .await
            .unwrap()
            .unwrap()
            .room_id,
        room_id
    );
    assert_eq!(
        timeout(Duration::from_millis(100), other_frames.recv())
            .await
            .unwrap()
            .unwrap()
            .room_id,
        other_room_id
    );
}

#[tokio::test]
async fn processed_media_late_subscriber_only_receives_future_frames() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user_01");
    start_relay_with_track(&state, room_id.clone(), user_id.clone(), NoiseProvider::Off);

    process_samples(&state, &room_id, &user_id, vec![1.0]);
    let mut frames = state.subscribe_processed_media_frames(&room_id);
    assert!(timeout(Duration::from_millis(25), frames.recv())
        .await
        .is_err());

    process_samples(&state, &room_id, &user_id, vec![2.0]);

    assert_eq!(
        timeout(Duration::from_millis(100), frames.recv())
            .await
            .unwrap()
            .unwrap()
            .samples,
        vec![2.0]
    );
    assert_eq!(state.processed_media_frames(&room_id).len(), 2);
}

#[tokio::test]
async fn media_relay_stop_route_clears_processed_frames() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let joined = state
        .registry
        .join(room_id.clone(), lyre_core::JoinRoomRequest::default());
    let user_id = joined.user.id.clone();
    start_relay_with_track(&state, room_id.clone(), user_id.clone(), NoiseProvider::Off);
    process_samples(&state, &room_id, &user_id, vec![1.0]);
    assert_eq!(state.processed_media_frames(&room_id).len(), 1);
    let app = router(state.clone());

    let response = app
        .oneshot(post_json_with_auth(
            "/api/rooms/DEFAULT/media-relay/stop",
            serde_json::json!({ "user_id": user_id }).to_string(),
            joined.access_token.as_str(),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(state.processed_media_frames(&room_id).is_empty());
}
