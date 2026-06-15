use crate::api::{router, AppState};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use lyre_core::{
    MediaRelayStatus, MediaTrackKind, RegisterMediaTrackRequest, RoomId, StartMediaRelayRequest,
    UserId,
};
use lyre_webrtc::{ServerMediaOffer, WebRtcStack};
use tower::ServiceExt;

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn post_json(uri: &str, body: String) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()
}

async fn offer_sdp() -> String {
    let offerer = WebRtcStack::new().create_peer_connection().await.unwrap();
    offerer.create_local_offer_for_test().await.unwrap()
}

async fn negotiate_server_media_for(state: &AppState, user_id: &str) {
    state
        .answer_server_media_offer(ServerMediaOffer {
            room_id: RoomId::default_room(),
            user_id: UserId::from_external(user_id),
            audio_track_id: "audio-main".to_owned(),
            sdp: offer_sdp().await,
        })
        .await
        .unwrap();
}

fn register_audio_track(state: &AppState, user_id: &str) {
    state
        .media_relays
        .register_track(
            RoomId::default_room(),
            RegisterMediaTrackRequest {
                user_id: UserId::from_external(user_id),
                track_id: "audio-main".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();
}

#[tokio::test]
async fn close_server_media_session_for_user_keeps_room_relay_and_other_peers() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    state
        .media_relays
        .start(room_id.clone(), StartMediaRelayRequest::default());
    register_audio_track(&state, "user_a");
    register_audio_track(&state, "user_b");
    negotiate_server_media_for(&state, "user_a").await;
    negotiate_server_media_for(&state, "user_b").await;

    let closed = state
        .close_server_media_session_for_user(room_id, UserId::from_external("user_a"))
        .unwrap();

    assert_eq!(closed.media_relay.status, MediaRelayStatus::Active);
    assert_eq!(closed.media_relay.participants.len(), 1);
    assert_eq!(
        closed.media_relay.participants[0].user_id.as_str(),
        "user_b"
    );
    assert_eq!(closed.session.unwrap().user_id.as_str(), "user_a");
    assert_eq!(state.server_media_peer_connection_count(), 1);
    assert_eq!(state.server_media_runtime_pump_count(), 1);
    assert_eq!(state.processed_audio_webrtc_egress_pump_count(), 0);
    assert_eq!(
        state.active_server_media_sessions()[0].user_id.as_str(),
        "user_b"
    );
}

#[tokio::test]
async fn server_media_close_route_returns_status_and_closed_session() {
    let state = AppState::default();
    state
        .media_relays
        .start(RoomId::default_room(), StartMediaRelayRequest::default());
    register_audio_track(&state, "user_01");
    negotiate_server_media_for(&state, "user_01").await;
    let app = router(state.clone());

    let response = app
        .oneshot(post_json(
            "/api/rooms/DEFAULT/server-media/close",
            serde_json::json!({ "user_id": "user_01" }).to_string(),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["media_relay"]["status"], "active");
    assert!(body["media_relay"]["participants"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(body["session"]["user_id"], "user_01");
    assert_eq!(body["session"]["state"], "closed");
    assert_eq!(state.server_media_peer_connection_count(), 0);
}

#[tokio::test]
async fn server_media_close_route_is_idempotent_for_missing_session() {
    let state = AppState::default();
    state
        .media_relays
        .start(RoomId::default_room(), StartMediaRelayRequest::default());
    register_audio_track(&state, "user_01");
    let app = router(state);

    let response = app
        .oneshot(post_json(
            "/api/rooms/DEFAULT/server-media/close",
            serde_json::json!({ "user_id": "user_01" }).to_string(),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["session"], serde_json::Value::Null);
    assert!(body["media_relay"]["participants"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn server_media_close_route_requires_active_relay_without_creating_room() {
    let state = AppState::default();
    let room_id = RoomId::parse_boundary("UNKNOWN").unwrap();
    let app = router(state.clone());

    let response = app
        .oneshot(post_json(
            "/api/rooms/UNKNOWN/server-media/close",
            serde_json::json!({ "user_id": "user_01" }).to_string(),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert!(body_json(response).await["error"]
        .as_str()
        .unwrap()
        .contains("media relay is not active"));
    assert!(!state.media_relays.contains_room(&room_id));
}
