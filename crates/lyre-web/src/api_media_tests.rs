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
use tower::ServiceExt;

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

fn post_json(uri: &str, body: &'static str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
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
async fn media_topology_route_documents_current_runtime_boundary() {
    let app = router(AppState::default());
    let response = app.oneshot(get("/api/webrtc/topology")).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["mode"], "p2p_mesh");
    assert_eq!(body["turn_relay_supported"], true);
    assert_eq!(body["server_side_audio_processing"], false);
    assert_eq!(body["server_side_noise_cancelling"], false);
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
    assert_eq!(body["mode"], "p2p_mesh");
    assert_eq!(body["server_side_audio_processing"], false);
    assert_eq!(body["server_side_noise_cancelling"], false);
    assert_eq!(body["noise"]["provider"], "off");
    assert!(body["participants"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn media_relay_register_track_requires_active_relay() {
    let app = router(AppState::default());
    let response = app
        .oneshot(post_json(
            "/api/rooms/DEFAULT/media-relay/tracks",
            r#"{"user_id":"user_01","track_id":"audio-main","kind":"audio"}"#,
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
    let start = app
        .clone()
        .oneshot(post_json(
            "/api/rooms/DEFAULT/media-relay/start",
            r#"{"noise":{"provider":"rnnoise","intensity":0.8,"voice_activity_threshold":0.2}}"#,
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
        .oneshot(post_json(
            "/api/rooms/DEFAULT/media-relay/tracks",
            r#"{"user_id":"user_01","track_id":"audio-main","kind":"audio"}"#,
        ))
        .await
        .unwrap();

    assert_eq!(register.status(), StatusCode::OK);
    let register_body = body_json(register).await;
    assert_eq!(register_body["participants"][0]["user_id"], "user_01");
    assert_eq!(
        register_body["participants"][0]["tracks"][0]["track_id"],
        "audio-main"
    );
    assert_eq!(
        register_body["participants"][0]["tracks"][0]["kind"],
        "audio"
    );

    let stop = app
        .oneshot(post_json(
            "/api/rooms/DEFAULT/media-relay/stop",
            r#"{"user_id":"user_01"}"#,
        ))
        .await
        .unwrap();

    assert_eq!(stop.status(), StatusCode::OK);
    let stop_body = body_json(stop).await;
    assert_eq!(stop_body["status"], "inactive");
    assert!(stop_body["participants"].as_array().unwrap().is_empty());
}

#[test]
fn app_state_process_media_frame_uses_shared_relay_state() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user_01");
    start_relay_with_track(&state, room_id.clone(), user_id.clone(), NoiseProvider::Off);

    process_samples(&state, &room_id, &user_id, vec![0.25, -0.5, 0.75]);

    let frames = state.processed_media_frames(&room_id);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].samples, vec![0.25, -0.5, 0.75]);
    assert_eq!(frames[0].noise.provider, NoiseProvider::Off);
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

#[test]
fn app_state_process_media_frame_runs_rnnoise_for_valid_audio() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user_01");
    start_relay_with_track(
        &state,
        room_id.clone(),
        user_id.clone(),
        NoiseProvider::Rnnoise,
    );

    process_samples(&state, &room_id, &user_id, vec![120.0; 480]);

    let frames = state.processed_media_frames(&room_id);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].samples.len(), 480);
    assert_eq!(frames[0].noise.provider, NoiseProvider::Rnnoise);
}

#[test]
fn app_state_process_media_frame_runs_deepfilternet_for_valid_audio() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user_01");
    start_relay_with_track(
        &state,
        room_id.clone(),
        user_id.clone(),
        NoiseProvider::Deepfilternet,
    );

    process_samples(&state, &room_id, &user_id, vec![120.0; 960]);

    let frames = state.processed_media_frames(&room_id);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].samples.len(), 960);
    assert!(frames[0].samples.iter().all(|sample| sample.is_finite()));
    assert_eq!(frames[0].noise.provider, NoiseProvider::Deepfilternet);
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
