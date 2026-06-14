# Server Media Negotiation Boundary Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a browser-to-server WebRTC offer/answer control-plane endpoint that creates a real server answer while keeping media packet handling out of scope.

**Architecture:** Keep all direct external `webrtc` crate usage in `crates/lyre-webrtc`. Add a new `negotiation.rs` owner that uses `WebRtcStack` and the shared `ServerMediaSessionRegistry`, stores live peer handles by room/user, and exposes only Lyre-owned DTOs/errors. Wire `lyre-web` to a REST endpoint and add frontend/WebRPC contract wrappers without changing the room UI runtime path.

**Tech Stack:** Rust workspace, `webrtc = "0.20.0-alpha.1"` inside `lyre-webrtc`, `dashmap`, `serde`, Axum REST, existing Next.js API helpers and Vitest.

---

### Task 1: Extend `lyre-webrtc` Session State and SDP Answering

**Files:**

- Modify: `crates/lyre-webrtc/Cargo.toml`
- Modify: `crates/lyre-webrtc/src/session.rs`
- Modify: `crates/lyre-webrtc/src/stack.rs`
- Modify: `crates/lyre-webrtc/src/lib.rs`

- [ ] **Step 1: Add serde dependency to `lyre-webrtc`**

In `crates/lyre-webrtc/Cargo.toml`, add:

```toml
serde.workspace = true
```

- [ ] **Step 2: Write failing session state tests**

Add these tests to `crates/lyre-webrtc/src/session.rs` inside the existing `tests` module:

```rust
#[test]
fn set_state_updates_existing_session() {
    let registry = ServerMediaSessionRegistry::new();
    let key = ServerMediaSessionKey {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
    };
    registry.start(config("DEFAULT", "user_01", "audio-main"));

    let status = registry
        .set_state(&key, ServerMediaSessionState::Negotiating)
        .unwrap();

    assert_eq!(status.state, ServerMediaSessionState::Negotiating);
    assert_eq!(
        registry.active_sessions()[0].state,
        ServerMediaSessionState::Negotiating
    );
}

#[test]
fn set_state_missing_session_returns_none() {
    let registry = ServerMediaSessionRegistry::new();

    assert_eq!(
        registry.set_state(
            &ServerMediaSessionKey {
                room_id: RoomId::default_room(),
                user_id: UserId::from_external("missing"),
            },
            ServerMediaSessionState::Negotiating,
        ),
        None
    );
}
```

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" -p lyre-webrtc set_state
```

Expected: fails because `set_state` does not exist yet.

- [ ] **Step 3: Implement state transition and serde derives**

In `crates/lyre-webrtc/src/session.rs`, import serde and update the public status enum/structs:

```rust
use serde::{Deserialize, Serialize};
```

Add derives:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerMediaSessionConfig {
    pub room_id: RoomId,
    pub user_id: UserId,
    pub audio_track_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerMediaSessionState {
    New,
    Negotiating,
    Connected,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerMediaSessionStatus {
    pub room_id: RoomId,
    pub user_id: UserId,
    pub audio_track_id: String,
    pub state: ServerMediaSessionState,
}
```

Add the method to `impl ServerMediaSessionRegistry`:

```rust
pub fn set_state(
    &self,
    key: &ServerMediaSessionKey,
    state: ServerMediaSessionState,
) -> Option<ServerMediaSessionStatus> {
    let mut session = self.sessions.get_mut(key)?;
    session.state = state;
    Some(status_from_session(key, &session))
}
```

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" -p lyre-webrtc set_state
```

Expected: both `set_state` tests pass.

- [ ] **Step 4: Write failing stack SDP answer tests**

Add this helper and tests to `crates/lyre-webrtc/src/stack.rs` inside the existing `tests` module:

```rust
async fn offer_sdp() -> String {
    let offerer = super::WebRtcStack::new()
        .create_peer_connection()
        .await
        .unwrap();
    let offer = offerer
        .create_local_offer_for_test()
        .await
        .unwrap();
    offer
}

#[tokio::test]
async fn answer_remote_offer_returns_answer_sdp() {
    let answerer = super::WebRtcStack::new()
        .create_peer_connection()
        .await
        .unwrap();

    let answer = answerer.answer_remote_offer(offer_sdp().await).await.unwrap();

    assert!(answer.starts_with("v=0"));
}

#[tokio::test]
async fn invalid_remote_offer_preserves_source_error() {
    let answerer = super::WebRtcStack::new()
        .create_peer_connection()
        .await
        .unwrap();

    let error = answerer
        .answer_remote_offer("not sdp".to_owned())
        .await
        .unwrap_err();

    assert!(std::error::Error::source(&error).is_some());
}
```

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" -p lyre-webrtc remote_offer
```

Expected: fails because the new methods do not exist yet.

- [ ] **Step 5: Implement SDP answer methods**

In `crates/lyre-webrtc/src/stack.rs`, import:

```rust
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
```

Add `Clone` to `WebRtcPeerConnectionHandle`:

```rust
#[derive(Clone)]
pub struct WebRtcPeerConnectionHandle {
    _peer_connection: Arc<dyn PeerConnection>,
}
```

Add methods:

```rust
impl WebRtcPeerConnectionHandle {
    pub async fn answer_remote_offer(&self, offer_sdp: String) -> Result<String, WebRtcStackError> {
        let offer = RTCSessionDescription::offer(offer_sdp).map_err(|source| {
            WebRtcStackError::CreateAnswer {
                source: Box::new(source),
            }
        })?;
        self._peer_connection
            .set_remote_description(offer)
            .await
            .map_err(|source| WebRtcStackError::CreateAnswer {
                source: Box::new(source),
            })?;
        let answer =
            self._peer_connection
                .create_answer(None)
                .await
                .map_err(|source| WebRtcStackError::CreateAnswer {
                    source: Box::new(source),
                })?;
        self._peer_connection
            .set_local_description(answer)
            .await
            .map_err(|source| WebRtcStackError::CreateAnswer {
                source: Box::new(source),
            })?;
        let local_description = self
            ._peer_connection
            .local_description()
            .await
            .ok_or(WebRtcStackError::MissingLocalDescription)?;
        Ok(local_description.sdp)
    }

    pub async fn create_local_offer_for_test(&self) -> Result<String, WebRtcStackError> {
        let offer =
            self._peer_connection
                .create_offer(None)
                .await
                .map_err(|source| WebRtcStackError::CreateOffer {
                    source: Box::new(source),
                })?;
        self._peer_connection
            .set_local_description(offer)
            .await
            .map_err(|source| WebRtcStackError::CreateOffer {
                source: Box::new(source),
            })?;
        let local_description = self
            ._peer_connection
            .local_description()
            .await
            .ok_or(WebRtcStackError::MissingLocalDescription)?;
        Ok(local_description.sdp)
    }

    pub(crate) fn debug_id(&self) -> usize {
        Arc::as_ptr(&self._peer_connection) as *const () as usize
    }
}
```

Extend `WebRtcStackError`:

```rust
#[error("failed to create WebRTC offer")]
CreateOffer {
    #[source]
    source: Box<dyn Error + Send + Sync>,
},
#[error("failed to create WebRTC answer")]
CreateAnswer {
    #[source]
    source: Box<dyn Error + Send + Sync>,
},
#[error("WebRTC peer connection did not produce a local description")]
MissingLocalDescription,
```

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" -p lyre-webrtc remote_offer
```

Expected: SDP answer tests pass. `create_local_offer_for_test` is a Lyre-owned SDP test helper that keeps `lyre-web` tests from importing the external `webrtc` crate directly. It returns only an SDP string and exposes no concrete `webrtc` types.

### Task 2: Add `ServerMediaNegotiator`

**Files:**

- Create: `crates/lyre-webrtc/src/negotiation.rs`
- Modify: `crates/lyre-webrtc/src/lib.rs`

- [ ] **Step 1: Write negotiator module and tests first**

Create `crates/lyre-webrtc/src/negotiation.rs` with the public DTOs, skeleton types, and tests:

```rust
use std::sync::Arc;

use dashmap::DashMap;
use lyre_core::{RoomId, UserId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    ServerMediaSessionConfig, ServerMediaSessionKey, ServerMediaSessionRegistry,
    ServerMediaSessionState, WebRtcPeerConnectionHandle, WebRtcStack, WebRtcStackError,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerMediaOffer {
    pub room_id: RoomId,
    pub user_id: UserId,
    pub audio_track_id: String,
    pub sdp: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerMediaAnswer {
    pub room_id: RoomId,
    pub user_id: UserId,
    pub audio_track_id: String,
    pub sdp: String,
    pub state: ServerMediaSessionState,
}

#[derive(Debug)]
pub struct ServerMediaNegotiator {
    stack: WebRtcStack,
    sessions: Arc<ServerMediaSessionRegistry>,
    peer_connections: DashMap<ServerMediaSessionKey, WebRtcPeerConnectionHandle>,
}

#[derive(Debug, Error)]
pub enum ServerMediaNegotiationError {
    #[error("failed to negotiate server media session")]
    WebRtc {
        #[source]
        source: WebRtcStackError,
    },
    #[error("server media session disappeared during negotiation")]
    SessionMissing,
}

impl ServerMediaNegotiator {
    pub fn new(stack: WebRtcStack, sessions: Arc<ServerMediaSessionRegistry>) -> Self {
        Self {
            stack,
            sessions,
            peer_connections: DashMap::new(),
        }
    }

    pub async fn answer_offer(
        &self,
        _offer: ServerMediaOffer,
    ) -> Result<ServerMediaAnswer, ServerMediaNegotiationError> {
        unimplemented!("answer_offer is implemented in the next plan step")
    }

    pub fn close(&self, key: &ServerMediaSessionKey) {
        self.sessions.close(key);
        self.peer_connections.remove(key);
    }

    pub fn close_room(&self, room_id: &RoomId) {
        self.sessions.close_room(room_id);
        self.peer_connections
            .retain(|key, _| &key.room_id != room_id);
    }

    pub fn stored_peer_connection_count(&self) -> usize {
        self.peer_connections.len()
    }

    #[cfg(test)]
    fn stored_peer_connection_debug_id(&self, key: &ServerMediaSessionKey) -> Option<usize> {
        self.peer_connections
            .get(key)
            .map(|entry| entry.value().debug_id())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn offer_sdp() -> String {
        let offerer = WebRtcStack::new()
            .create_peer_connection()
            .await
            .unwrap();
        offerer.create_local_offer_for_test().await.unwrap()
    }

    fn offer(track: &str, sdp: String) -> ServerMediaOffer {
        ServerMediaOffer {
            room_id: RoomId::default_room(),
            user_id: UserId::from_external("user_01"),
            audio_track_id: track.to_owned(),
            sdp,
        }
    }

    #[tokio::test]
    async fn answer_offer_marks_session_negotiating_and_stores_handle() {
        let sessions = Arc::new(ServerMediaSessionRegistry::new());
        let negotiator = ServerMediaNegotiator::new(WebRtcStack::new(), Arc::clone(&sessions));

        let answer = negotiator.answer_offer(offer("audio-main", offer_sdp().await)).await.unwrap();

        assert!(answer.sdp.starts_with("v=0"));
        assert_eq!(answer.state, ServerMediaSessionState::Negotiating);
        assert_eq!(sessions.active_sessions()[0].state, ServerMediaSessionState::Negotiating);
        assert_eq!(negotiator.stored_peer_connection_count(), 1);
    }

    #[tokio::test]
    async fn failed_offer_does_not_create_session_or_handle() {
        let sessions = Arc::new(ServerMediaSessionRegistry::new());
        let negotiator = ServerMediaNegotiator::new(WebRtcStack::new(), Arc::clone(&sessions));

        let result = negotiator.answer_offer(offer("audio-main", "not sdp".to_owned())).await;

        assert!(result.is_err());
        assert!(sessions.sessions().is_empty());
        assert_eq!(negotiator.stored_peer_connection_count(), 0);
    }

    #[tokio::test]
    async fn repeated_successful_offer_replaces_track_and_handle() {
        let sessions = Arc::new(ServerMediaSessionRegistry::new());
        let negotiator = ServerMediaNegotiator::new(WebRtcStack::new(), Arc::clone(&sessions));
        let key = ServerMediaSessionKey {
            room_id: RoomId::default_room(),
            user_id: UserId::from_external("user_01"),
        };

        negotiator.answer_offer(offer("audio-main", offer_sdp().await)).await.unwrap();
        let first_handle = negotiator.stored_peer_connection_debug_id(&key).unwrap();
        negotiator.answer_offer(offer("audio-retry", offer_sdp().await)).await.unwrap();
        let second_handle = negotiator.stored_peer_connection_debug_id(&key).unwrap();

        assert_eq!(sessions.sessions().len(), 1);
        assert_eq!(sessions.sessions()[0].audio_track_id, "audio-retry");
        assert_eq!(negotiator.stored_peer_connection_count(), 1);
        assert_ne!(first_handle, second_handle);
    }

    #[tokio::test]
    async fn failed_renegotiation_preserves_existing_session_and_handle() {
        let sessions = Arc::new(ServerMediaSessionRegistry::new());
        let negotiator = ServerMediaNegotiator::new(WebRtcStack::new(), Arc::clone(&sessions));
        let key = ServerMediaSessionKey {
            room_id: RoomId::default_room(),
            user_id: UserId::from_external("user_01"),
        };
        negotiator.answer_offer(offer("audio-main", offer_sdp().await)).await.unwrap();
        let status_before = sessions.sessions();
        let handle_before = negotiator.stored_peer_connection_debug_id(&key).unwrap();

        let result = negotiator.answer_offer(offer("audio-retry", "not sdp".to_owned())).await;

        assert!(result.is_err());
        assert_eq!(sessions.sessions(), status_before);
        assert_eq!(negotiator.stored_peer_connection_debug_id(&key), Some(handle_before));
        assert_eq!(negotiator.stored_peer_connection_count(), 1);
    }

    #[tokio::test]
    async fn close_and_close_room_remove_stored_handles() {
        let sessions = Arc::new(ServerMediaSessionRegistry::new());
        let negotiator = ServerMediaNegotiator::new(WebRtcStack::new(), Arc::clone(&sessions));
        negotiator.answer_offer(offer("audio-main", offer_sdp().await)).await.unwrap();

        negotiator.close(&ServerMediaSessionKey {
            room_id: RoomId::default_room(),
            user_id: UserId::from_external("user_01"),
        });

        assert_eq!(negotiator.stored_peer_connection_count(), 0);

        negotiator.answer_offer(offer("audio-main", offer_sdp().await)).await.unwrap();
        negotiator.close_room(&RoomId::default_room());

        assert_eq!(negotiator.stored_peer_connection_count(), 0);
    }
}
```

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" -p lyre-webrtc negotiation
```

Expected: fails because `answer_offer` is still `unimplemented!`.

- [ ] **Step 2: Implement negotiator**

Replace `unimplemented!` in `answer_offer` with:

```rust
let peer_connection = self
    .stack
    .create_peer_connection()
    .await
    .map_err(|source| ServerMediaNegotiationError::WebRtc { source })?;
let answer_sdp = peer_connection
    .answer_remote_offer(offer.sdp)
    .await
    .map_err(|source| ServerMediaNegotiationError::WebRtc { source })?;

let key = ServerMediaSessionKey {
    room_id: offer.room_id.clone(),
    user_id: offer.user_id.clone(),
};
self.sessions.start(ServerMediaSessionConfig {
    room_id: offer.room_id.clone(),
    user_id: offer.user_id.clone(),
    audio_track_id: offer.audio_track_id.clone(),
});
let status = self
    .sessions
    .set_state(&key, ServerMediaSessionState::Negotiating)
    .ok_or(ServerMediaNegotiationError::SessionMissing)?;
self.peer_connections.insert(key, peer_connection);

Ok(ServerMediaAnswer {
    room_id: status.room_id,
    user_id: status.user_id,
    audio_track_id: status.audio_track_id,
    sdp: answer_sdp,
    state: status.state,
})
```

Update `crates/lyre-webrtc/src/lib.rs`:

```rust
pub mod negotiation;
pub mod session;
pub mod stack;

pub use negotiation::{
    ServerMediaAnswer, ServerMediaNegotiationError, ServerMediaNegotiator, ServerMediaOffer,
};
```

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" -p lyre-webrtc negotiation
```

Expected: negotiator tests pass.

### Task 3: Wire REST API and Error Mapping

**Files:**

- Modify: `crates/lyre-web/src/error.rs`
- Modify: `crates/lyre-web/src/api.rs`
- Create: `crates/lyre-web/src/api_server_media_tests.rs`
- Modify: `crates/lyre-web/src/lib.rs`

- [ ] **Step 1: Write failing API tests**

Create `crates/lyre-web/src/api_server_media_tests.rs`:

```rust
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
    let offerer = WebRtcStack::new()
        .create_peer_connection()
        .await
        .unwrap();
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
    assert_eq!(state.server_media_sessions()[0].room_id, RoomId::default_room());
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
```

Add the test module to `crates/lyre-web/src/lib.rs`:

```rust
#[cfg(test)]
mod api_server_media_tests;
```

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" -p lyre-web server_media_offer
```

Expected: fails because the route and AppState negotiator are not wired.

- [ ] **Step 2: Extend API error mapping**

In `crates/lyre-web/src/error.rs`, import:

```rust
use std::error::Error;

use lyre_webrtc::ServerMediaNegotiationError;
```

Add enum variant and `From`:

```rust
ServerMediaNegotiation(ServerMediaNegotiationError),
```

```rust
impl From<ServerMediaNegotiationError> for ApiError {
    fn from(error: ServerMediaNegotiationError) -> Self {
        Self::ServerMediaNegotiation(error)
    }
}
```

Add a local helper to render explicit source chains:

```rust
fn error_chain(error: &dyn Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(error) = source {
        message.push_str(": ");
        message.push_str(&error.to_string());
        source = error.source();
    }
    message
}
```

In `IntoResponse`, map WebRTC answer errors to `BAD_REQUEST` and other negotiation errors to `INTERNAL_SERVER_ERROR`. Use `error_chain(&error)` for the JSON message so the response explicitly preserves lower-level causes required by the repo error policy:

```rust
Self::ServerMediaNegotiation(error) => {
    let status = match &error {
        ServerMediaNegotiationError::WebRtc {
            source: lyre_webrtc::WebRtcStackError::CreateAnswer { .. },
        } => StatusCode::BAD_REQUEST,
        ServerMediaNegotiationError::WebRtc { .. }
        | ServerMediaNegotiationError::SessionMissing => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, error_chain(&error))
},
```

- [ ] **Step 3: Wire AppState and REST route**

In `crates/lyre-web/src/api.rs`, update imports:

```rust
use lyre_webrtc::{
    ServerMediaAnswer, ServerMediaNegotiationError, ServerMediaNegotiator, ServerMediaOffer,
    ServerMediaSessionConfig, ServerMediaSessionRegistry, ServerMediaSessionStatus, WebRtcStack,
};
```

Add field:

```rust
pub server_media_negotiator: Arc<ServerMediaNegotiator>,
```

In `AppState::new`, create the session registry first and share it:

```rust
let server_media_sessions = Arc::new(ServerMediaSessionRegistry::new());
let server_media_negotiator = Arc::new(ServerMediaNegotiator::new(
    WebRtcStack::new(),
    Arc::clone(&server_media_sessions),
));
```

Store both fields. Add methods:

```rust
pub async fn answer_server_media_offer(
    &self,
    offer: ServerMediaOffer,
) -> Result<ServerMediaAnswer, ServerMediaNegotiationError> {
    self.server_media_negotiator.answer_offer(offer).await
}

#[cfg(test)]
pub fn server_media_peer_connection_count(&self) -> usize {
    self.server_media_negotiator.stored_peer_connection_count()
}
```

Change `close_server_media_sessions_for_room` to use the negotiator cleanup path:

```rust
pub fn close_server_media_sessions_for_room(
    &self,
    room_id: &RoomId,
) -> Vec<ServerMediaSessionStatus> {
    self.server_media_negotiator.close_room(room_id);
    self.server_media_sessions.sessions()
}
```

Add request DTO:

```rust
#[derive(Debug, Deserialize)]
struct ServerMediaOfferRequest {
    user_id: lyre_core::UserId,
    audio_track_id: String,
    sdp: String,
}
```

Add route:

```rust
.route(
    "/api/rooms/{room_id}/server-media/offer",
    post(answer_server_media_offer),
)
```

Add handler:

```rust
async fn answer_server_media_offer(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(request): Json<ServerMediaOfferRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    let answer = state
        .answer_server_media_offer(ServerMediaOffer {
            room_id,
            user_id: request.user_id,
            audio_track_id: request.audio_track_id,
            sdp: request.sdp,
        })
        .await?;
    Ok(Json(answer))
}
```

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" -p lyre-web server_media_offer
```

Expected: API tests pass.

### Task 4: Update WebRPC Contract and Frontend API Wrapper

**Files:**

- Modify: `proto/lyre.ridl`
- Modify: `frontend/src/lib/api.ts`
- Modify: `frontend/src/lib/api.test.ts`
- Regenerate: `frontend/src/lib/lyre.gen.ts`

- [ ] **Step 1: Extend RIDL contract**

In `proto/lyre.ridl`, add:

```ridl
enum ServerMediaSessionState: uint32
  - NEW = 0
  - NEGOTIATING = 1
  - CONNECTED = 2
  - CLOSED = 3

struct ServerMediaOfferInput
  - userID: string
  - audioTrackID: string
  - sdp: string

struct ServerMediaAnswer
  - roomID: string
  - userID: string
  - audioTrackID: string
  - sdp: string
  - state: ServerMediaSessionState
```

Add service method:

```ridl
# Documents POST /api/rooms/{room_id}/server-media/offer; REST fetch remains the runtime transport in this increment.
- AnswerServerMediaOffer(roomID: string, userID: string, audioTrackID: string, sdp: string) => (answer: ServerMediaAnswer)
```

- [ ] **Step 2: Update frontend API wrapper tests**

Run `cd frontend && npm run generate:webrpc` before editing tests so `ServerMediaSessionState` and `ServerMediaAnswer` exist in `lyre.gen.ts`. Then in `frontend/src/lib/api.test.ts`, import `answerServerMediaOffer`, `serverMediaOfferUrl`, REST `type ServerMediaAnswer`, generated `type ServerMediaAnswer as WebrpcServerMediaAnswer`, and generated `ServerMediaSessionState`. Add shape tests:

```ts
const serverMediaAnswerFromRestShape: ServerMediaAnswer = {
  room_id: "DEFAULT",
  user_id: "user_a",
  audio_track_id: "audio-main",
  sdp: "v=0",
  state: "negotiating"
};
void serverMediaAnswerFromRestShape;

const generatedServerMediaAnswerContract: WebrpcServerMediaAnswer = {
  roomID: "DEFAULT",
  userID: "user_a",
  audioTrackID: "audio-main",
  sdp: "v=0",
  state: ServerMediaSessionState.NEGOTIATING
};
void generatedServerMediaAnswerContract;
```

Add tests:

```ts
it("builds encoded server media offer urls", () => {
  expect(serverMediaOfferUrl("Team A")).toBe(
    "https://api.example.test/api/rooms/Team%20A/server-media/offer"
  );
});

it("serializes server media offer request body", async () => {
  await answerServerMediaOffer("DEFAULT", "user_a", "audio-main", "v=0");

  expect(fetch).toHaveBeenCalledWith("https://api.example.test/api/rooms/DEFAULT/server-media/offer", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ user_id: "user_a", audio_track_id: "audio-main", sdp: "v=0" })
  });
});
```

Run:

```bash
cd frontend && npm run generate:webrpc && npm test -- --run src/lib/api.test.ts
```

Expected: generated contract types exist, then the test fails until the REST wrapper is implemented.

- [ ] **Step 3: Implement frontend wrapper**

In `frontend/src/lib/api.ts`, import generated answer/session types from `lyre.gen`:

```ts
import type {
  ServerMediaAnswer as WebrpcServerMediaAnswer,
  IceServerConfig as WebrpcIceServerConfig,
  JoinRoomInput as WebrpcJoinRoomInput,
  JoinRoomResponse as WebrpcJoinRoomResponse,
  MediaRelayParticipant as WebrpcMediaRelayParticipant,
  MediaRelayRoomStatus as WebrpcMediaRelayRoomStatus,
  MediaRelayTrack as WebrpcMediaRelayTrack,
  MediaTopology as WebrpcMediaTopology,
  NoiseCancellationConfig as WebrpcNoiseCancellationConfig,
  RoomSnapshot as WebrpcRoomSnapshot,
  UserProfile as WebrpcUserProfile
} from "./lyre.gen";
```

Add:

```ts
export type ServerMediaSessionState = "new" | "negotiating" | "connected" | "closed";

export type ServerMediaAnswer = Omit<
  WebrpcServerMediaAnswer,
  "roomID" | "userID" | "audioTrackID" | "state"
> & {
  room_id: string;
  user_id: string;
  audio_track_id: string;
  state: ServerMediaSessionState;
};

export function serverMediaOfferUrl(roomId: string): string {
  return `${roomUrl(roomId)}/server-media/offer`;
}

export async function answerServerMediaOffer(
  roomId: string,
  userId: string,
  audioTrackId: string,
  sdp: string
): Promise<ServerMediaAnswer> {
  const response = await fetch(serverMediaOfferUrl(roomId), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ user_id: userId, audio_track_id: audioTrackId, sdp })
  });
  return response.json();
}
```

Do not map generated enum values at runtime for REST responses in this increment. The generated contract check proves RIDL shape; the `ServerMediaAnswer` REST type and `serverMediaAnswerFromRestShape` test prove snake_case runtime behavior.

Regenerate:

```bash
cd frontend && npm run generate:webrpc
```

Run:

```bash
cd frontend && npm test -- --run src/lib/api.test.ts
```

Expected: API wrapper tests pass.

### Task 5: Documentation and Verification

**Files:**

- Modify: `MEMORY.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Update memory**

Append to `MEMORY.md`:

```markdown
## 2026-06-15 Server Media Negotiation Boundary

- Added a server media offer/answer control-plane path that creates real WebRTC answers inside `lyre-webrtc`.
- Kept negotiation atomic: failed offers do not create sessions or replace stored peer handles.
- Stored peer connection handles only to keep negotiated sessions alive for later media work; RTP/RTCP, Opus, RNNoise ingestion, and browser playback remain future work.
```

- [ ] **Step 2: Update roadmap**

In `docs/roadmap.md`, add to Completed:

```markdown
- Server media WebRTC offer/answer negotiation boundary.
```

Keep the Next items for real media termination, RNNoise wiring, and processed audio broadcast.

- [ ] **Step 3: Run targeted verification**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" -p lyre-webrtc remote_offer negotiation set_state
cargo nextest run --manifest-path "Cargo.toml" -p lyre-web server_media_offer
cd frontend && npm test -- --run src/lib/api.test.ts
```

Expected: all targeted checks pass.

- [ ] **Step 4: Run full verification**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
cd frontend && npm run generate:webrpc && npm test -- --run && npm run typecheck && npm run lint && npm run build
git diff --check
rg -n "webrtc::" crates/lyre-core crates/lyre-web crates/lyre-webrtc/src Cargo.toml crates/*/Cargo.toml
rg -n "pub (use|type) .*webrtc|pub [^\\n]*: .*webrtc::|pub (async )?fn [^\\n]*webrtc::" crates/lyre-webrtc/src
wc -l crates/lyre-webrtc/src/*.rs crates/lyre-web/src/*.rs
```

Expected:

- Rust and frontend checks exit 0.
- Wide `webrtc::` grep shows only private implementation usage in `crates/lyre-webrtc` plus internal `lyre_webrtc` crate path/test string noise.
- Public API leak grep exits 1 with no matches.
- No modified Rust source file exceeds 400 LOC.

- [ ] **Step 5: Independent implementation review, commit, and push**

After implementation and targeted verification, produce a review patch that includes untracked files:

```bash
git diff -- . ':!frontend/.next' > /tmp/server-media-negotiation.patch
```

Dispatch an independent implementation reviewer with the approved spec, this plan, the patch, and verification output. Proceed only after `VERDICT: APPROVE`.

After docs and full verification pass, commit with Lore format:

```text
Add server media offer answer boundary

Create a real server-side WebRTC answer path without claiming media termination so future RTP/Opus processing has a stable control-plane entry.

Constraint: Direct external webrtc crate usage stays isolated to lyre-webrtc.
Rejected: Switching the room UI to server media relay in this increment | negotiation alone does not provide RTP/Opus processing or playback.
Confidence: high
Scope-risk: moderate
Directive: Keep P2P mesh as the default frontend path until server media RTP/RTCP, Opus, noise processing, and playback are implemented.
Tested: cargo fmt --all --check; cargo clippy --workspace --all-targets -- -D warnings; cargo nextest run --manifest-path Cargo.toml --workspace; frontend generate:webrpc/test/typecheck/lint/build; git diff --check; static webrtc dependency/public API leak checks
Not-tested: DTLS-SRTP media termination, RTP/RTCP forwarding, Opus decode/encode, RNNoise ingestion from WebRTC tracks, browser playback of processed server audio
```

Push:

```bash
git push origin main
```
