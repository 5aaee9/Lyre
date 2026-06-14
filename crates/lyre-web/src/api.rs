use crate::{
    error::ApiError,
    signalling::{route_signal_message, PeerHub, SignalMessage, SignalPayload},
};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use lyre_core::{
    default_ice_servers, supported_noise_providers, IceServerConfig, JoinRoomRequest,
    LeaveRoomRequest, RoomId, RoomRegistry,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

#[derive(Debug, Clone)]
pub struct AppState {
    pub registry: Arc<RoomRegistry>,
    pub peers: Arc<PeerHub>,
    pub ice_servers: Arc<Vec<IceServerConfig>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new(default_ice_servers())
    }
}

impl AppState {
    pub fn new(ice_servers: Vec<IceServerConfig>) -> Self {
        Self {
            registry: Arc::new(RoomRegistry::new()),
            peers: Arc::new(PeerHub::new()),
            ice_servers: Arc::new(ice_servers),
        }
    }
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Debug, Deserialize)]
struct WsQuery {
    user_id: String,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/noise/providers", get(noise_providers))
        .route("/api/webrtc/ice-servers", get(ice_servers))
        .route("/api/rooms/{room_id}", get(room_snapshot))
        .route("/api/rooms/{room_id}/join", post(join_room))
        .route("/api/rooms/{room_id}/leave", post(leave_room))
        .route("/api/rooms/{room_id}/ws", get(room_ws))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn noise_providers() -> Json<Vec<lyre_core::NoiseCancellationConfig>> {
    Json(supported_noise_providers())
}

async fn ice_servers(State(state): State<AppState>) -> Json<Vec<IceServerConfig>> {
    Json((*state.ice_servers).clone())
}

async fn room_snapshot(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    Ok(Json(state.registry.snapshot(room_id)))
}

async fn join_room(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(request): Json<JoinRoomRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    let response = state.registry.join(room_id.clone(), request);
    state.peers.user_joined(&room_id, response.user.clone());
    Ok((StatusCode::CREATED, Json(response)))
}

async fn leave_room(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(request): Json<LeaveRoomRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    let snapshot = state.registry.leave(&room_id, &request.user_id);
    state.peers.user_left(&room_id, &request.user_id);
    Ok(Json(snapshot))
}

async fn room_ws(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    let user_id = lyre_core::UserId::from_external(query.user_id);
    Ok(ws.on_upgrade(move |socket| handle_socket(state, room_id, user_id, socket)))
}

async fn handle_socket(
    state: AppState,
    room_id: RoomId,
    user_id: lyre_core::UserId,
    socket: WebSocket,
) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (peer_tx, mut peer_rx) = mpsc::unbounded_channel();
    let snapshot = state
        .peers
        .connect(&state.registry, room_id.clone(), user_id.clone(), peer_tx);
    let snapshot_message = SignalMessage::new(
        room_id.clone(),
        user_id.clone(),
        Some(user_id.clone()),
        SignalPayload::RoomSnapshot { room: snapshot },
    );
    let _ = ws_tx
        .send(Message::Text(
            serde_json::to_string(&snapshot_message).unwrap().into(),
        ))
        .await;

    loop {
        tokio::select! {
            Some(message) = peer_rx.recv() => {
                match serde_json::to_string(&message) {
                    Ok(json) => {
                        if ws_tx.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        tracing::debug!(error = %error, "failed to serialize signal message");
                    }
                }
            }
            Some(Ok(message)) = ws_rx.next() => {
                if let Message::Text(text) = message {
                    match serde_json::from_str::<SignalMessage>(&text) {
                        Ok(signal) => match route_signal_message(&room_id, &user_id, &signal) {
                            Ok(_) => {
                                state.peers.forward(signal);
                            }
                            Err(error) => {
                                let _ = ws_tx.send(Message::Text(serde_json::to_string(&*error).unwrap().into())).await;
                            }
                        },
                        Err(_) => {
                            let error = SignalMessage::error(room_id.clone(), user_id.clone(), "invalid signal message");
                            let _ = ws_tx.send(Message::Text(serde_json::to_string(&error).unwrap().into())).await;
                        }
                    }
                }
            }
            else => break,
        }
    }

    state.peers.disconnect(&room_id, &user_id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
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
        let app = router(AppState::new(vec![
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
        ]));
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
}
