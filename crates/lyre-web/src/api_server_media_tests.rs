use crate::api::{router, AppState};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use lyre_core::RoomId;
use lyre_webrtc::WebRtcStack;
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

#[tokio::test]
async fn server_media_offer_route_returns_answer_and_updates_shared_sessions() {
    let state = AppState::default();
    let app = router(state.clone());
    let response = app
        .oneshot(post_json(
            "/api/rooms/DEFAULT/server-media/offer",
            serde_json::json!({
                "room_id": "IGNORED",
                "user_id": "user_01",
                "audio_track_id": "audio-main",
                "sdp": offer_sdp().await,
            })
            .to_string(),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["room_id"], "DEFAULT");
    assert_eq!(body["user_id"], "user_01");
    assert_eq!(body["audio_track_id"], "audio-main");
    assert_eq!(body["state"], "negotiating");
    assert!(body["sdp"].as_str().unwrap().starts_with("v=0"));
    assert_eq!(
        state.server_media_sessions()[0].room_id,
        RoomId::default_room()
    );
    assert_eq!(state.server_media_peer_connection_count(), 1);
}

#[tokio::test]
async fn server_media_offer_route_rejects_invalid_sdp_without_session() {
    let state = AppState::default();
    let app = router(state.clone());
    let response = app
        .oneshot(post_json(
            "/api/rooms/DEFAULT/server-media/offer",
            serde_json::json!({
                "user_id": "user_01",
                "audio_track_id": "audio-main",
                "sdp": "not sdp",
            })
            .to_string(),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(body_json(response).await["error"]
        .as_str()
        .unwrap()
        .contains("failed to create WebRTC answer"));
    assert!(state.server_media_sessions().is_empty());
    assert_eq!(state.server_media_peer_connection_count(), 0);
}

#[tokio::test]
async fn stopping_media_relay_removes_server_media_peer_handle() {
    let state = AppState::default();
    let app = router(state.clone());
    let response = app
        .clone()
        .oneshot(post_json(
            "/api/rooms/DEFAULT/server-media/offer",
            serde_json::json!({
                "user_id": "user_01",
                "audio_track_id": "audio-main",
                "sdp": offer_sdp().await,
            })
            .to_string(),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(state.server_media_peer_connection_count(), 1);

    let stop = app
        .oneshot(post_json(
            "/api/rooms/DEFAULT/media-relay/stop",
            serde_json::json!({ "user_id": "user_01" }).to_string(),
        ))
        .await
        .unwrap();

    assert_eq!(stop.status(), StatusCode::OK);
    assert_eq!(state.server_media_peer_connection_count(), 0);
}
