use crate::api::{router, AppState};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use lyre_core::{
    AudioFrame, MediaRelayError, MediaTrackKind, NoiseCancellationConfig, NoiseProvider,
    RegisterMediaTrackRequest, RoomId, StartMediaRelayRequest, StopMediaRelayRequest, UserId,
};
use tokio::time::{timeout, Duration};
use tower::ServiceExt;

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

fn post_json(uri: &str, body: impl Into<Body>) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(body.into())
        .unwrap()
}

fn post_json_with_auth(uri: &str, body: String, access_token: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access_token}"))
        .body(Body::from(body))
        .unwrap()
}

async fn join_for_test(app: axum::Router, nickname: &str) -> (String, String) {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rooms/DEFAULT/join")
                .header("content-type", "application/json")
                .body(Body::from(format!(r#"{{"nickname":"{nickname}"}}"#)))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = body_json(response).await;
    (
        body["user"]["id"].as_str().unwrap().to_owned(),
        body["access_token"].as_str().unwrap().to_owned(),
    )
}

fn audio_frame(room_id: RoomId, user_id: UserId, samples: Vec<f32>) -> AudioFrame {
    AudioFrame {
        room_id,
        user_id,
        track_id: "audio-main".to_owned(),
        sample_rate_hz: 48_000,
        channels: 1,
        sequence: 1,
        rtp_timestamp: None,
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
                ..NoiseCancellationConfig::default()
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
async fn media_topology_route_documents_current_runtime_boundary() {
    let app = router(AppState::default());
    let response = app.oneshot(get("/api/webrtc/topology")).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["mode"], "media_relay");
    assert_eq!(body["turn_relay_supported"], true);
    assert_eq!(body["server_side_audio_processing"], true);
    assert_eq!(body["server_side_noise_cancelling"], true);
    assert_eq!(body["server_noise_cancelling_requires"], "media_relay");
}

#[tokio::test]
async fn media_relay_status_defaults_to_inactive() {
    let app = router(AppState::default());
    let response = app
        .oneshot(get("/api/rooms/DEFAULT/media-relay"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["room_id"], "DEFAULT");
    assert_eq!(body["status"], "inactive");
    assert_eq!(body["mode"], "media_relay");
    assert_eq!(body["server_side_audio_processing"], false);
    assert_eq!(body["server_side_noise_cancelling"], false);
    assert_eq!(body["noise"]["provider"], "off");
    assert!(body["participants"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn media_relay_register_track_requires_active_relay() {
    let app = router(AppState::default());
    let (user_id, access_token) = join_for_test(app.clone(), "Alice").await;
    let response = app
        .oneshot(post_json_with_auth(
            "/api/rooms/DEFAULT/media-relay/tracks",
            serde_json::json!({
                "user_id": user_id,
                "track_id": "audio-main",
                "kind": "audio",
            })
            .to_string(),
            &access_token,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        body_json(response).await["error"],
        "media relay is not active for room `DEFAULT`"
    );
}

#[tokio::test]
async fn media_relay_start_registers_track_and_stop_clears_state() {
    let app = router(AppState::default());
    let (user_id, access_token) = join_for_test(app.clone(), "Alice").await;
    let start = app
        .clone()
        .oneshot(post_json_with_auth(
            "/api/rooms/DEFAULT/media-relay/start",
            r#"{"noise":{"provider":"rnnoise","intensity":0.8,"voice_activity_threshold":0.2}}"#
                .to_owned(),
            &access_token,
        ))
        .await
        .unwrap();

    assert_eq!(start.status(), StatusCode::OK);
    let start_body = body_json(start).await;
    assert_eq!(start_body["status"], "active");
    assert_eq!(start_body["mode"], "media_relay");
    assert_eq!(start_body["noise"]["provider"], "rnnoise");
    assert_eq!(start_body["server_side_audio_processing"], false);

    let register = app
        .clone()
        .oneshot(post_json_with_auth(
            "/api/rooms/DEFAULT/media-relay/tracks",
            serde_json::json!({
                "user_id": user_id,
                "track_id": "audio-main",
                "kind": "audio",
            })
            .to_string(),
            &access_token,
        ))
        .await
        .unwrap();

    assert_eq!(register.status(), StatusCode::OK);
    let register_body = body_json(register).await;
    assert_eq!(register_body["participants"][0]["user_id"], user_id);
    assert_eq!(
        register_body["participants"][0]["tracks"][0]["track_id"],
        "audio-main"
    );
    assert_eq!(
        register_body["participants"][0]["tracks"][0]["kind"],
        "audio"
    );

    let stop = app
        .oneshot(post_json_with_auth(
            "/api/rooms/DEFAULT/media-relay/stop",
            serde_json::json!({ "user_id": user_id }).to_string(),
            &access_token,
        ))
        .await
        .unwrap();

    assert_eq!(stop.status(), StatusCode::OK);
    let stop_body = body_json(stop).await;
    assert_eq!(stop_body["status"], "inactive");
    assert!(stop_body["participants"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn media_relay_settings_update_switches_noise_on_and_off() {
    let state = AppState::default();
    let app = router(state.clone());
    let (user_id, access_token) = join_for_test(app.clone(), "Alice").await;
    app.clone()
        .oneshot(post_json_with_auth(
            "/api/rooms/DEFAULT/media-relay/start",
            r#"{"noise":{"provider":"off","intensity":0.5,"voice_activity_threshold":0.35}}"#
                .to_owned(),
            &access_token,
        ))
        .await
        .unwrap();
    app.clone()
        .oneshot(post_json_with_auth(
            "/api/rooms/DEFAULT/media-relay/tracks",
            serde_json::json!({
                "user_id": user_id,
                "track_id": "audio-main",
                "kind": "audio",
            })
            .to_string(),
            &access_token,
        ))
        .await
        .unwrap();

    let rnnoise = app
        .clone()
        .oneshot(post_json_with_auth(
            "/api/rooms/DEFAULT/media-relay/settings",
            serde_json::json!({
                "user_id": user_id,
                "noise": {
                    "provider": "rnnoise",
                    "intensity": 0.7,
                    "voice_activity_threshold": 0.25
                }
            })
            .to_string(),
            &access_token,
        ))
        .await
        .unwrap();

    assert_eq!(rnnoise.status(), StatusCode::OK);
    let rnnoise_body = body_json(rnnoise).await;
    assert_eq!(rnnoise_body["noise"]["provider"], "rnnoise");
    assert_eq!(
        state
            .media_relays
            .require_track(
                &RoomId::default_room(),
                &UserId::from_external(&user_id),
                "audio-main"
            )
            .unwrap()
            .noise
            .provider,
        NoiseProvider::Rnnoise
    );

    let off = app
        .oneshot(post_json_with_auth(
            "/api/rooms/DEFAULT/media-relay/settings",
            serde_json::json!({
                "user_id": user_id,
                "noise": {
                    "provider": "off",
                    "intensity": 0.5,
                    "voice_activity_threshold": 0.35
                }
            })
            .to_string(),
            &access_token,
        ))
        .await
        .unwrap();

    assert_eq!(off.status(), StatusCode::OK);
    let off_body = body_json(off).await;
    assert_eq!(off_body["noise"]["provider"], "off");
    assert_eq!(
        state
            .media_relays
            .require_track(
                &RoomId::default_room(),
                &UserId::from_external(&user_id),
                "audio-main"
            )
            .unwrap()
            .noise
            .provider,
        NoiseProvider::Off
    );
}

#[tokio::test]
async fn media_relay_start_requires_bearer_token() {
    let app = router(AppState::default());

    let response = app
        .oneshot(post_json(
            "/api/rooms/DEFAULT/media-relay/start",
            r#"{"noise":{"provider":"off","intensity":0.5,"voice_activity_threshold":0.35}}"#,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        body_json(response).await["error"],
        "room access token is invalid"
    );
}

#[tokio::test]
async fn media_relay_subscriptions_require_bearer_token_for_user_id() {
    let state = AppState::default();
    let app = router(state.clone());
    let (user_id, _access_token) = join_for_test(app.clone(), "Alice").await;
    let room_id = RoomId::default_room();
    start_relay_with_track(
        &state,
        room_id,
        UserId::from_external(&user_id),
        NoiseProvider::Off,
    );

    let response = app
        .oneshot(post_json(
            "/api/rooms/DEFAULT/media-relay/subscriptions",
            serde_json::json!({
                "user_id": user_id,
                "source_user_ids": [],
            })
            .to_string(),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        body_json(response).await["error"],
        "room access token is invalid"
    );
}

#[tokio::test]
async fn media_relay_subscriptions_reject_unknown_source_users() {
    let state = AppState::default();
    let app = router(state.clone());
    let (user_id, access_token) = join_for_test(app.clone(), "Alice").await;
    let room_id = RoomId::default_room();
    start_relay_with_track(
        &state,
        room_id,
        UserId::from_external(&user_id),
        NoiseProvider::Off,
    );

    let response = app
        .oneshot(post_json_with_auth(
            "/api/rooms/DEFAULT/media-relay/subscriptions",
            serde_json::json!({
                "user_id": user_id,
                "source_user_ids": ["missing"],
            })
            .to_string(),
            &access_token,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        body_json(response).await["error"],
        "media relay participant `missing` is not registered in room `DEFAULT`"
    );
}

#[tokio::test]
async fn media_relay_subscriptions_accept_empty_source_list() {
    let state = AppState::default();
    let app = router(state.clone());
    let (user_id, access_token) = join_for_test(app.clone(), "Alice").await;
    let room_id = RoomId::default_room();
    start_relay_with_track(
        &state,
        room_id,
        UserId::from_external(&user_id),
        NoiseProvider::Off,
    );

    let response = app
        .oneshot(post_json_with_auth(
            "/api/rooms/DEFAULT/media-relay/subscriptions",
            serde_json::json!({
                "user_id": user_id,
                "source_user_ids": [],
            })
            .to_string(),
            &access_token,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["room_id"], "DEFAULT");
    assert_eq!(body["user_id"], user_id);
    assert_eq!(body["source_user_ids"], serde_json::json!([]));
}

#[tokio::test]
async fn media_relay_subscriptions_sort_and_deduplicate_source_users() {
    let state = AppState::default();
    let app = router(state.clone());
    let (alice_id, alice_token) = join_for_test(app.clone(), "Alice").await;
    let (bob_id, _bob_token) = join_for_test(app.clone(), "Bob").await;
    let (carol_id, _carol_token) = join_for_test(app.clone(), "Carol").await;
    let room_id = RoomId::default_room();
    state
        .media_relays
        .start(room_id.clone(), StartMediaRelayRequest::default());
    for user_id in [&alice_id, &bob_id, &carol_id] {
        state
            .media_relays
            .register_track(
                room_id.clone(),
                RegisterMediaTrackRequest {
                    user_id: UserId::from_external(user_id),
                    track_id: "audio-main".to_owned(),
                    kind: MediaTrackKind::Audio,
                },
            )
            .unwrap();
    }

    let response = app
        .oneshot(post_json_with_auth(
            "/api/rooms/DEFAULT/media-relay/subscriptions",
            serde_json::json!({
                "user_id": alice_id,
                "source_user_ids": [carol_id, bob_id, bob_id, alice_id],
            })
            .to_string(),
            &alice_token,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let mut expected_source_user_ids = vec![bob_id, carol_id];
    expected_source_user_ids.sort();
    assert_eq!(body["room_id"], "DEFAULT");
    assert_eq!(body["user_id"], alice_id);
    assert_eq!(
        body["source_user_ids"],
        serde_json::json!(expected_source_user_ids)
    );
}

#[tokio::test]
async fn app_state_process_media_frame_uses_shared_relay_state() {
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
    assert_eq!(frame.noise.provider, NoiseProvider::Off);
}

#[test]
fn app_state_process_media_frame_propagates_relay_errors() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user_01");

    assert_eq!(
        state.process_media_frame(audio_frame(room_id.clone(), user_id.clone(), vec![0.0])),
        Err(MediaRelayError::Inactive {
            room_id: room_id.clone(),
        })
    );

    state
        .media_relays
        .start(room_id.clone(), StartMediaRelayRequest::default());
    assert_eq!(
        state.process_media_frame(audio_frame(room_id.clone(), user_id.clone(), vec![0.0])),
        Err(MediaRelayError::ParticipantNotFound {
            room_id: room_id.clone(),
            user_id: user_id.clone(),
        })
    );

    state
        .media_relays
        .register_track(
            room_id.clone(),
            RegisterMediaTrackRequest {
                user_id: user_id.clone(),
                track_id: "other-track".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();
    assert_eq!(
        state.process_media_frame(audio_frame(room_id.clone(), user_id.clone(), vec![0.0])),
        Err(MediaRelayError::TrackNotFound {
            room_id,
            user_id,
            track_id: "audio-main".to_owned(),
        })
    );
}

#[test]
fn app_state_process_media_frame_does_not_create_unknown_room() {
    let state = AppState::default();
    let room_id = RoomId::parse_boundary("UNKNOWN").unwrap();

    assert_eq!(
        state.process_media_frame(audio_frame(
            room_id.clone(),
            UserId::from_external("user_01"),
            vec![0.0],
        )),
        Err(MediaRelayError::Inactive {
            room_id: room_id.clone(),
        })
    );
    assert!(!state.media_relays.contains_room(&room_id));
}

#[tokio::test]
async fn app_state_process_media_frame_runs_rnnoise_for_valid_audio() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user_01");
    start_relay_with_track(
        &state,
        room_id.clone(),
        user_id.clone(),
        NoiseProvider::Rnnoise,
    );
    let mut frames = state.subscribe_processed_media_frames(&room_id);

    process_samples(&state, &room_id, &user_id, vec![120.0; 480]);

    let frame = timeout(Duration::from_millis(100), frames.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(frame.samples.len(), 480);
    assert_eq!(frame.noise.provider, NoiseProvider::Rnnoise);
}

#[tokio::test]
async fn app_state_process_media_frame_runs_deepfilternet_for_valid_audio() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user_01");
    start_relay_with_track(
        &state,
        room_id.clone(),
        user_id.clone(),
        NoiseProvider::Deepfilternet,
    );
    let mut frames = state.subscribe_processed_media_frames(&room_id);

    process_samples(&state, &room_id, &user_id, vec![120.0; 960]);

    let frame = timeout(Duration::from_millis(100), frames.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(frame.samples.len(), 960);
    assert!(frame.samples.iter().all(|sample| sample.is_finite()));
    assert_eq!(frame.noise.provider, NoiseProvider::Deepfilternet);
}

#[test]
fn app_state_process_media_frame_stop_relay_prevents_future_processing() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user_01");
    start_relay_with_track(&state, room_id.clone(), user_id.clone(), NoiseProvider::Off);

    state.media_relays.stop(
        room_id.clone(),
        StopMediaRelayRequest {
            user_id: user_id.clone(),
        },
    );

    assert_eq!(
        state.process_media_frame(audio_frame(room_id.clone(), user_id, vec![0.0])),
        Err(MediaRelayError::Inactive { room_id })
    );
}
