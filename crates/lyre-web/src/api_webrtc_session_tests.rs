use crate::api::{router, AppState};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use lyre_core::{RoomId, StopMediaRelayRequest, UserId};
use lyre_webrtc::{ServerMediaSessionConfig, ServerMediaSessionState, ServerMediaSessionStatus};
use tower::ServiceExt;

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
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

fn session_config(room_id: RoomId, user_id: UserId) -> ServerMediaSessionConfig {
    ServerMediaSessionConfig {
        room_id,
        user_id,
        audio_track_id: "audio-main".to_owned(),
    }
}

#[test]
fn app_state_exposes_shared_webrtc_session_registry() {
    let state = AppState::default();
    let status = state.start_server_media_session(session_config(
        RoomId::default_room(),
        UserId::from_external("user_01"),
    ));

    assert_eq!(status.state, ServerMediaSessionState::New);
    assert_eq!(status.audio_track_id, "audio-main");
    assert_eq!(state.active_server_media_sessions(), vec![status]);
}

#[test]
fn stop_media_relay_closes_webrtc_sessions_for_room() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user_01");
    state.start_server_media_session(session_config(room_id.clone(), user_id.clone()));

    state.stop_media_relay(
        room_id.clone(),
        StopMediaRelayRequest {
            user_id: user_id.clone(),
        },
    );

    assert_eq!(
        state.server_media_sessions(),
        vec![ServerMediaSessionStatus {
            room_id,
            user_id,
            audio_track_id: "audio-main".to_owned(),
            state: ServerMediaSessionState::Closed,
        }]
    );
    assert!(state.active_server_media_sessions().is_empty());
}

#[tokio::test]
async fn media_relay_stop_route_closes_webrtc_sessions_for_room() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let joined = state
        .registry
        .join(room_id.clone(), lyre_core::JoinRoomRequest::default());
    let user_id = joined.user.id.clone();
    state.start_server_media_session(session_config(room_id, user_id.clone()));
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
    assert_eq!(body_json(response).await["status"], "inactive");
    assert!(state.active_server_media_sessions().is_empty());
}
