use axum::{body::Body, http::Request};
use http_body_util::BodyExt;

mod errors;
mod media_relay;
mod rooms;
mod server_media;

pub(super) async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

pub(super) fn rpc_post(method: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(format!("/rpc/Lyre/{method}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

pub(super) fn rpc_post_auth(
    method: &str,
    body: serde_json::Value,
    access_token: &str,
) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(format!("/rpc/Lyre/{method}"))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access_token}"))
        .body(Body::from(body.to_string()))
        .unwrap()
}

pub(super) async fn offer_sdp() -> String {
    let offerer = lyre_webrtc::WebRtcStack::new()
        .create_peer_connection()
        .await
        .unwrap();
    offerer.create_local_offer_for_test().await.unwrap()
}
