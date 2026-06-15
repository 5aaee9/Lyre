use super::{body_json, rpc_post};
use crate::api::{router, AppState};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

#[tokio::test]
async fn webrpc_malformed_json_returns_error_envelope() {
    let app = router(AppState::default());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/rpc/Lyre/GetRoom")
                .header("content-type", "application/json")
                .body(Body::from("{not-json"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = body_json(response).await;
    assert_eq!(body["name"], "WebrpcEndpoint");
    assert_eq!(body["message"], "bad request");
    assert!(body.get("error").is_none());
}

#[tokio::test]
async fn webrpc_unknown_method_returns_plain_404() {
    let app = router(AppState::default());
    let response = app
        .oneshot(rpc_post("NoSuchMethod", serde_json::json!({})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
