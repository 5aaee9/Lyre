use crate::api::{router, AppState};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use lyre_core::{RoomId, UserId};
use lyre_noise_cancelling::DeepFilterNetRuntimeConfig;
use lyre_webrtc::{ServerMediaSessionKey, WebRtcStack};
use tower::ServiceExt;

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[derive(Debug)]
struct JoinedForTest {
    user_id: String,
    access_token: String,
}

fn post_json(uri: &str, body: String) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body))
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

async fn join_for_test(app: axum::Router, nickname: &str) -> JoinedForTest {
    let response = app
        .oneshot(post_json(
            "/api/rooms/DEFAULT/join",
            serde_json::json!({ "nickname": nickname }).to_string(),
        ))
        .await
        .unwrap();
    let body = body_json(response).await;
    JoinedForTest {
        user_id: body["user"]["id"].as_str().unwrap().to_owned(),
        access_token: body["access_token"].as_str().unwrap().to_owned(),
    }
}

async fn offer_sdp() -> String {
    let offerer = WebRtcStack::new().create_peer_connection().await.unwrap();
    offerer.create_local_offer_for_test().await.unwrap()
}

async fn negotiate_server_media(state: &AppState) -> JoinedForTest {
    let app = router(state.clone());
    let joined = join_for_test(app.clone(), "Alice").await;
    let response = app
        .oneshot(post_json_with_auth(
            "/api/rooms/DEFAULT/server-media/offer",
            serde_json::json!({
                "user_id": joined.user_id,
                "audio_track_id": "audio-main",
                "sdp": offer_sdp().await,
            })
            .to_string(),
            &joined.access_token,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    joined
}

fn candidate_body(user_id: &str) -> String {
    serde_json::json!({
        "room_id": "IGNORED",
        "user_id": user_id,
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
    let joined = join_for_test(app.clone(), "Alice").await;
    let response = app
        .oneshot(post_json_with_auth(
            "/api/rooms/DEFAULT/server-media/offer",
            serde_json::json!({
                "room_id": "IGNORED",
                "user_id": joined.user_id,
                "audio_track_id": "audio-main",
                "sdp": offer_sdp().await,
            })
            .to_string(),
            &joined.access_token,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["room_id"], "DEFAULT");
    assert_eq!(body["user_id"], joined.user_id);
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
    let joined = join_for_test(app.clone(), "Alice").await;
    let response = app
        .oneshot(post_json_with_auth(
            "/api/rooms/DEFAULT/server-media/offer",
            serde_json::json!({
                "user_id": joined.user_id,
                "audio_track_id": "audio-main",
                "sdp": "not sdp",
            })
            .to_string(),
            &joined.access_token,
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
    let joined = join_for_test(app.clone(), "Alice").await;
    let response = app
        .clone()
        .oneshot(post_json_with_auth(
            "/api/rooms/DEFAULT/server-media/offer",
            serde_json::json!({
                "user_id": joined.user_id,
                "audio_track_id": "audio-main",
                "sdp": offer_sdp().await,
            })
            .to_string(),
            &joined.access_token,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(state.server_media_peer_connection_count(), 1);

    let stop = app
        .oneshot(post_json_with_auth(
            "/api/rooms/DEFAULT/media-relay/stop",
            serde_json::json!({ "user_id": joined.user_id }).to_string(),
            &joined.access_token,
        ))
        .await
        .unwrap();

    assert_eq!(stop.status(), StatusCode::OK);
    assert_eq!(state.server_media_peer_connection_count(), 0);
}

#[tokio::test]
async fn server_media_candidate_route_accepts_existing_peer_candidate() {
    let state = AppState::default();
    let joined = negotiate_server_media(&state).await;
    let app = router(state);

    let response = app
        .oneshot(post_json_with_auth(
            "/api/rooms/DEFAULT/server-media/candidates",
            candidate_body(&joined.user_id),
            &joined.access_token,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["room_id"], "DEFAULT");
    assert_eq!(body["user_id"], joined.user_id);
    assert_eq!(
        body["candidate"],
        "candidate:1 1 UDP 2130706431 192.168.1.100 54321 typ host"
    );
}

#[tokio::test]
async fn server_media_candidate_route_rejects_missing_peer() {
    let state = AppState::default();
    let app = router(state.clone());
    let joined = join_for_test(app.clone(), "Alice").await;

    let response = app
        .oneshot(post_json_with_auth(
            "/api/rooms/DEFAULT/server-media/candidates",
            candidate_body(&joined.user_id),
            &joined.access_token,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert!(state.server_media_sessions().is_empty());
}

#[tokio::test]
async fn server_media_candidates_route_lists_server_candidates() {
    let state = AppState::default();
    let joined = negotiate_server_media(&state).await;
    let app = router(state);

    let mut body = serde_json::Value::Null;
    for _ in 0..128 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/api/rooms/DEFAULT/server-media/candidates?user_id={}",
                        joined.user_id
                    ))
                    .header("authorization", format!("Bearer {}", joined.access_token))
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
            && candidate["user_id"] == joined.user_id
            && candidate["candidate"]
                .as_str()
                .unwrap()
                .starts_with("candidate:")
    }));
    assert!(body.as_array().unwrap().iter().any(|candidate| {
        candidate["room_id"] == "DEFAULT"
            && candidate["user_id"] == joined.user_id
            && candidate["candidate"] == ""
    }));
}

#[tokio::test]
async fn server_media_candidates_route_uses_configured_public_ip() {
    let state = AppState::with_room_state_persistence_and_server_media_public_ip(
        lyre_core::default_ice_servers(),
        None,
        None,
        DeepFilterNetRuntimeConfig::default(),
        Some("203.0.113.10".parse().unwrap()),
    )
    .unwrap();
    let joined = negotiate_server_media(&state).await;
    let app = router(state);

    let mut body = serde_json::Value::Null;
    for _ in 0..128 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/api/rooms/DEFAULT/server-media/candidates?user_id={}",
                        joined.user_id
                    ))
                    .header("authorization", format!("Bearer {}", joined.access_token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        body = body_json(response).await;
        if body.as_array().unwrap().iter().any(|candidate| {
            candidate["candidate"]
                .as_str()
                .unwrap()
                .contains(" 203.0.113.10 ")
        }) {
            break;
        }
        tokio::task::yield_now().await;
    }

    assert!(body.as_array().unwrap().iter().any(|candidate| {
        candidate["candidate"]
            .as_str()
            .unwrap()
            .contains(" 203.0.113.10 ")
    }));
}

#[tokio::test]
async fn server_media_offer_requires_bearer_token() {
    let state = AppState::default();
    let app = router(state);

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

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn server_media_candidates_reject_token_for_different_user() {
    let state = AppState::default();
    let app = router(state);
    let first = join_for_test(app.clone(), "Alice").await;
    let second = join_for_test(app.clone(), "Bob").await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/rooms/DEFAULT/server-media/candidates?user_id={}",
                    second.user_id
                ))
                .header("authorization", format!("Bearer {}", first.access_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
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
    assert!(state.drain_server_media_pcm_frames(&key).is_empty());
    assert!(state.drain_server_media_decode_failures(&key).is_empty());
    assert_eq!(state.process_server_media_pcm_frames(&key), Ok(0));
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

#[tokio::test]
async fn server_media_pcm_frames_route_does_not_exist() {
    let app = router(AppState::default());

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/rooms/DEFAULT/server-media/pcm-frames?user_id=user_01")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn server_media_runtime_pump_route_does_not_exist() {
    let app = router(AppState::default());

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/rooms/DEFAULT/server-media/pump?user_id=user_01")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn server_media_decode_failures_route_does_not_exist() {
    let app = router(AppState::default());

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/rooms/DEFAULT/server-media/decode-failures?user_id=user_01")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn server_media_debug_route_does_not_exist() {
    let app = router(AppState::default());

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/rooms/DEFAULT/server-media/debug?user_id=user_01")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn server_media_egress_routes_do_not_exist() {
    let app = router(AppState::default());

    for uri in [
        "/api/rooms/DEFAULT/server-media/egress?user_id=user_01",
        "/api/rooms/DEFAULT/server-media/egress-packets?user_id=user_01",
        "/api/rooms/DEFAULT/server-media/encode-failures?user_id=user_01",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
