use crate::api::{router, AppState};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use tower::ServiceExt;

async fn response_text(response: axum::response::Response) -> String {
    String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap()
}

fn metric_value(body: &str, name: &str) -> u64 {
    body.lines()
        .find_map(|line| {
            let (metric, value) = line.split_once(' ')?;
            (metric == name).then(|| value.parse().unwrap())
        })
        .unwrap()
}

#[tokio::test]
async fn metrics_route_returns_prometheus_text() {
    let app = router(AppState::default());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"],
        "text/plain; version=0.0.4"
    );
    let body = response_text(response).await;
    assert!(body.contains("# TYPE lyre_rooms_total gauge"));
    assert!(body.contains("# TYPE lyre_room_joins_total counter"));
    assert!(body.contains("lyre_room_state_persistence_failures_total 0"));
    assert!(!body.contains("DEFAULT"));
    assert!(!body.contains("access_token"));
}

#[tokio::test]
async fn metrics_track_join_and_leave_counts() {
    let app = router(AppState::default());
    let join = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rooms/DEFAULT/join")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"nickname":"Ada"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let join_body: serde_json::Value =
        serde_json::from_slice(&join.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let user_id = join_body["user"]["id"].as_str().unwrap();
    let access_token = join_body["access_token"].as_str().unwrap();

    let before_leave = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response_text(before_leave).await;
    assert_eq!(metric_value(&body, "lyre_rooms_total"), 1);
    assert_eq!(metric_value(&body, "lyre_users_total"), 1);
    assert_eq!(metric_value(&body, "lyre_room_joins_total"), 1);
    assert_eq!(metric_value(&body, "lyre_room_leaves_total"), 0);

    let leave = app
        .clone()
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
    assert_eq!(leave.status(), StatusCode::OK);

    let after_leave = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response_text(after_leave).await;
    assert_eq!(metric_value(&body, "lyre_rooms_total"), 1);
    assert_eq!(metric_value(&body, "lyre_users_total"), 0);
    assert_eq!(metric_value(&body, "lyre_room_joins_total"), 1);
    assert_eq!(metric_value(&body, "lyre_room_leaves_total"), 1);
}

#[tokio::test]
async fn metrics_route_does_not_create_room_entries() {
    let state = AppState::default();
    let app = router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(state.registry.aggregate().rooms, 0);
}
