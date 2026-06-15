use crate::api::{router, AppState};
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use lyre_core::IceServerConfig;
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

#[derive(Debug)]
struct JoinedForTest {
    user_id: String,
    access_token: String,
}

#[derive(Clone)]
struct CapturedWriter(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for CapturedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn join_for_test(app: Router, nickname: &str) -> JoinedForTest {
    join_room_for_test(app, "DEFAULT", nickname).await
}

async fn join_room_for_test(app: Router, room_id: &str, nickname: &str) -> JoinedForTest {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/rooms/{room_id}/join"))
                .header("content-type", "application/json")
                .body(Body::from(format!(r#"{{"nickname":"{nickname}"}}"#)))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = body_json(response).await;
    JoinedForTest {
        user_id: body["user"]["id"].as_str().unwrap().to_owned(),
        access_token: body["access_token"].as_str().unwrap().to_owned(),
    }
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
    let access_token = join_body["access_token"].as_str().unwrap();

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
                .header("authorization", format!("Bearer {access_token}"))
                .body(Body::from(format!(r#"{{"user_id":"{user_id}"}}"#)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(body_json(leave).await["users"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn protected_room_leave_requires_bearer_token() {
    let app = router(AppState::default());
    let joined = join_for_test(app.clone(), "Alice").await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rooms/DEFAULT/leave")
                .header("content-type", "application/json")
                .body(Body::from(format!(r#"{{"user_id":"{}"}}"#, joined.user_id)))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
}

#[tokio::test]
async fn protected_room_leave_rejects_token_for_different_user() {
    let app = router(AppState::default());
    let first = join_for_test(app.clone(), "Alice").await;
    let second = join_for_test(app.clone(), "Bob").await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rooms/DEFAULT/leave")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", first.access_token))
                .body(Body::from(format!(r#"{{"user_id":"{}"}}"#, second.user_id)))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
}

#[tokio::test]
async fn protected_room_leave_rejects_malformed_unknown_and_room_mismatched_tokens() {
    let app = router(AppState::default());
    let default = join_for_test(app.clone(), "Alice").await;
    let other = join_room_for_test(app.clone(), "OTHER", "Bob").await;

    for authorization in [
        "Token not-bearer".to_owned(),
        "Bearer unknown-token".to_owned(),
        format!("Bearer {}", other.access_token),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/rooms/DEFAULT/leave")
                    .header("content-type", "application/json")
                    .header("authorization", authorization)
                    .body(Body::from(format!(
                        r#"{{"user_id":"{}"}}"#,
                        default.user_id
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            body_json(response).await["error"],
            "room access token is invalid"
        );
    }
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
    assert_eq!(body.as_array().unwrap().len(), 4);
    assert_eq!(body[3]["provider"], "dpdfnet");
    assert_eq!(body[3]["dpdfnet"]["model"], "dpdfnet2_48khz_hr");
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

#[test]
fn request_trace_path_redacts_query_tokens() {
    let uri: axum::http::Uri = "/api/rooms/DEFAULT/ws?user_id=user_01&access_token=secret"
        .parse()
        .unwrap();
    assert_eq!(
        crate::api::redacted_trace_path(&uri),
        "/api/rooms/DEFAULT/ws"
    );
}

#[tokio::test]
async fn websocket_route_requires_access_token_query() {
    let app = router(AppState::default());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/rooms/DEFAULT/ws?user_id=user_01")
                .header("connection", "upgrade")
                .header("upgrade", "websocket")
                .header("sec-websocket-version", "13")
                .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
}

#[tokio::test]
async fn websocket_request_trace_does_not_log_access_token_query() {
    let logs = Arc::new(Mutex::new(Vec::<u8>::new()));
    let writer = CapturedWriter(Arc::clone(&logs));
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || writer.clone())
        .with_ansi(false)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let app = router(AppState::default());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/rooms/DEFAULT/ws?user_id=user_01&access_token=secret-token")
                .header("connection", "upgrade")
                .header("upgrade", "websocket")
                .header("sec-websocket-version", "13")
                .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
    let output = String::from_utf8(logs.lock().unwrap().clone()).unwrap();
    assert!(!output.contains("secret-token"));
    assert!(!output.contains("access_token"));
}
