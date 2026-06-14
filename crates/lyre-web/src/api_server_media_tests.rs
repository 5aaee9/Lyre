use crate::api::{router, AppState};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use lyre_core::{RoomId, UserId};
use lyre_webrtc::{ServerMediaSessionKey, WebRtcStack};
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

async fn negotiate_server_media(state: &AppState) {
    let app = router(state.clone());
    let response = app
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
}

fn candidate_body() -> String {
    serde_json::json!({
        "room_id": "IGNORED",
        "user_id": "user_01",
        "candidate": "candidate:1 1 UDP 2130706431 192.168.1.100 54321 typ host",
        "sdp_mid": "0",
        "sdp_mline_index": 0,
        "username_fragment": null,
    })
    .to_string()
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

#[tokio::test]
async fn server_media_candidate_route_accepts_existing_peer_candidate() {
    let state = AppState::default();
    negotiate_server_media(&state).await;
    let app = router(state);

    let response = app
        .oneshot(post_json(
            "/api/rooms/DEFAULT/server-media/candidates",
            candidate_body(),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["room_id"], "DEFAULT");
    assert_eq!(body["user_id"], "user_01");
    assert_eq!(
        body["candidate"],
        "candidate:1 1 UDP 2130706431 192.168.1.100 54321 typ host"
    );
}

#[tokio::test]
async fn server_media_candidate_route_rejects_missing_peer() {
    let state = AppState::default();
    let app = router(state.clone());

    let response = app
        .oneshot(post_json(
            "/api/rooms/DEFAULT/server-media/candidates",
            candidate_body(),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(state.server_media_sessions().is_empty());
}

#[tokio::test]
async fn server_media_candidates_route_lists_server_candidates() {
    let state = AppState::default();
    negotiate_server_media(&state).await;
    let app = router(state);

    let mut body = serde_json::Value::Null;
    for _ in 0..128 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/rooms/DEFAULT/server-media/candidates?user_id=user_01")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        body = body_json(response).await;
        let candidates = body.as_array().unwrap();
        if candidates.iter().any(|candidate| {
            candidate["candidate"]
                .as_str()
                .unwrap()
                .starts_with("candidate:")
        }) && candidates
            .iter()
            .any(|candidate| candidate["candidate"] == "")
        {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(body.as_array().unwrap().iter().any(|candidate| {
        candidate["room_id"] == "DEFAULT"
            && candidate["user_id"] == "user_01"
            && candidate["candidate"]
                .as_str()
                .unwrap()
                .starts_with("candidate:")
    }));
    assert!(body.as_array().unwrap().iter().any(|candidate| {
        candidate["room_id"] == "DEFAULT"
            && candidate["user_id"] == "user_01"
            && candidate["candidate"] == ""
    }));
}

#[tokio::test]
async fn app_state_server_media_snapshots_are_internal_and_empty_for_missing_session() {
    let state = AppState::default();
    let key = ServerMediaSessionKey {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
    };

    assert!(state.server_media_remote_tracks(&key).is_empty());
    assert!(state.server_media_received_rtp_packets(&key).is_empty());
}

#[tokio::test]
async fn server_media_raw_rtp_packets_route_does_not_exist() {
    let app = router(AppState::default());

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/rooms/DEFAULT/server-media/rtp-packets?user_id=user_01")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
