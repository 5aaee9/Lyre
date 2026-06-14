use crate::api::{router, AppState};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use lyre_core::IceServerConfig;
use tower::ServiceExt;

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn health_route_returns_ok() {
    let app = router(AppState::default());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await["status"], "ok");
}

#[tokio::test]
async fn room_routes_join_snapshot_and_leave() {
    let app = router(AppState::default());
    let join = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rooms/DEFAULT/join")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"nickname":"Alice"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(join.status(), StatusCode::CREATED);
    let join_body = body_json(join).await;
    let user_id = join_body["user"]["id"].as_str().unwrap();

    let snapshot = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/rooms/DEFAULT")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        body_json(snapshot).await["users"].as_array().unwrap().len(),
        1
    );

    let leave = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rooms/DEFAULT/leave")
                .header("content-type", "application/json")
                .body(Body::from(format!(r#"{{"user_id":"{user_id}"}}"#)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(body_json(leave).await["users"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn noise_provider_route_returns_supported_providers() {
    let app = router(AppState::default());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/noise/providers")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = body_json(response).await;
    assert_eq!(body.as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn ice_server_route_returns_default_servers() {
    let app = router(AppState::default());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/webrtc/ice-servers")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = body_json(response).await;
    assert_eq!(body[0]["urls"][0], "stun:stun.l.google.com:19302");
}

#[tokio::test]
async fn ice_server_route_preserves_configured_servers() {
    let app = router(AppState::new(
        vec![
            IceServerConfig {
                urls: vec!["stun:one.example:3478".to_owned()],
                username: None,
                credential: None,
            },
            IceServerConfig {
                urls: vec!["stun:one.example:3478".to_owned()],
                username: Some("user".to_owned()),
                credential: Some("pass".to_owned()),
            },
        ],
        None,
    ));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/webrtc/ice-servers")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = body_json(response).await;
    assert_eq!(body.as_array().unwrap().len(), 2);
    assert_eq!(body[1]["username"], "user");
}

#[tokio::test]
async fn ice_server_route_generates_short_lived_turn_credentials() {
    let app = router(AppState::new(
        vec![IceServerConfig {
            urls: vec!["turn:turn.example:3478".to_owned()],
            username: Some("static-user".to_owned()),
            credential: Some("static-pass".to_owned()),
        }],
        Some(lyre_core::TurnRestCredentialsConfig {
            secret: "turn-secret".to_owned(),
            ttl_seconds: 3600,
            identity: "lyre".to_owned(),
        }),
    ));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/webrtc/ice-servers")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = body_json(response).await;
    assert_eq!(body[0]["urls"][0], "turn:turn.example:3478");
    assert!(body[0]["username"].as_str().unwrap().ends_with(":lyre"));
    assert_ne!(body[0]["credential"], "static-pass");
    assert!(!body.to_string().contains("turn-secret"));
}

#[tokio::test]
async fn route_rejects_blank_room_id() {
    let app = router(AppState::default());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/rooms/%20%20")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn media_relay_route_rejects_blank_room_id() {
    let app = router(AppState::default());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/rooms/%20%20/media-relay")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn malformed_leave_body_is_client_error() {
    let app = router(AppState::default());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rooms/DEFAULT/leave")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(response.status().is_client_error());
}
