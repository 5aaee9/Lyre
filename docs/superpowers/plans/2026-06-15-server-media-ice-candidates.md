# Server Media ICE Candidate Exchange Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add HTTP and Rust control-plane support for exchanging ICE candidates on already negotiated server media WebRTC peer connections.

**Architecture:** Keep direct `webrtc` crate usage inside `crates/lyre-webrtc`. Add Lyre-owned candidate DTOs, convert to/from `RTCIceCandidateInit` only in `stack.rs`, and let `ServerMediaNegotiator` own candidate add/query for stored peer handles. Move server-media HTTP handlers into a focused `api_server_media.rs` module so `api.rs` stays below the 400 LOC split threshold.

**Tech Stack:** Rust workspace, `webrtc = "0.20.0-alpha.1"` inside `lyre-webrtc`, Axum REST, serde DTOs, WebRPC RIDL TypeScript generation, Vitest.

---

### Task 1: Add Candidate DTOs and PeerConnection Candidate Methods

**Files:**

- Modify: `Cargo.toml`
- Modify: `crates/lyre-webrtc/Cargo.toml`
- Modify: `crates/lyre-webrtc/src/stack.rs`
- Modify: `crates/lyre-webrtc/src/lib.rs`

- [ ] **Step 1: Add async-trait dependency for the WebRTC event handler**

In root `Cargo.toml`, add the workspace dependency:

```toml
async-trait = "0.1"
```

In `crates/lyre-webrtc/Cargo.toml`, add:

```toml
async-trait.workspace = true
```

- [ ] **Step 2: Write failing stack candidate tests**

Add these helpers and tests to `crates/lyre-webrtc/src/stack.rs` inside the existing `tests` module:

```rust
fn host_candidate() -> super::ServerMediaIceCandidateInit {
    super::ServerMediaIceCandidateInit {
        candidate: "candidate:1 1 UDP 2130706431 192.168.1.100 54321 typ host".to_owned(),
        sdp_mid: Some("0".to_owned()),
        sdp_mline_index: Some(0),
        username_fragment: None,
    }
}

async fn wait_for_local_candidates(
    handle: &super::WebRtcPeerConnectionHandle,
) -> Vec<super::ServerMediaIceCandidateInit> {
    for _ in 0..128 {
        let candidates = handle.local_ice_candidates();
        if candidates.iter().any(|candidate| candidate.candidate.starts_with("candidate:"))
            && candidates.iter().any(|candidate| candidate.candidate.is_empty())
        {
            return candidates;
        }
        tokio::task::yield_now().await;
    }
    handle.local_ice_candidates()
}

#[tokio::test]
async fn add_remote_ice_candidate_accepts_candidate_after_answer() {
    let answerer = super::WebRtcStack::new()
        .create_peer_connection()
        .await
        .unwrap();
    answerer
        .answer_remote_offer(offer_sdp().await)
        .await
        .unwrap();

    answerer.add_remote_ice_candidate(host_candidate()).await.unwrap();
}

#[tokio::test]
async fn invalid_remote_ice_candidate_preserves_source_error() {
    let answerer = super::WebRtcStack::new()
        .create_peer_connection()
        .await
        .unwrap();
    answerer
        .answer_remote_offer(offer_sdp().await)
        .await
        .unwrap();
    let mut candidate = host_candidate();
    candidate.candidate = "not a candidate".to_owned();

    let error = answerer.add_remote_ice_candidate(candidate).await.unwrap_err();

    assert!(std::error::Error::source(&error).is_some());
}

#[tokio::test]
async fn local_ice_candidates_are_lyre_owned_values() {
    let answerer = super::WebRtcStack::new()
        .create_peer_connection()
        .await
        .unwrap();
    answerer
        .answer_remote_offer(offer_sdp().await)
        .await
        .unwrap();

    let candidates = wait_for_local_candidates(&answerer).await;

    assert!(candidates.iter().any(|candidate| candidate.candidate.starts_with("candidate:")));
    assert!(candidates.iter().any(|candidate| candidate.candidate.is_empty()));
}
```

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" -p lyre-webrtc ice_candidate local_ice
```

Expected: fails because candidate DTOs and methods do not exist.

- [ ] **Step 3: Implement Lyre-owned candidate DTOs and conversion**

In `crates/lyre-webrtc/src/stack.rs`, add imports:

```rust
use std::{error::Error, sync::{Arc, Mutex}};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
    RTCIceCandidateInit, RTCIceGatheringState, RTCPeerConnectionIceEvent,
    RTCSessionDescription,
};
```

Replace the noop handler with a collecting handler:

```rust
#[derive(Clone)]
struct PeerConnectionHandler {
    local_ice_candidates: Arc<Mutex<Vec<ServerMediaIceCandidateInit>>>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for PeerConnectionHandler {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        let candidate = match event.candidate.to_json() {
            Ok(candidate) => candidate,
            Err(_) => return,
        };
        self.local_ice_candidates
            .lock()
            .expect("local ICE candidate collection lock must not be poisoned")
            .push(ServerMediaIceCandidateInit::from(candidate));
    }

    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            self.local_ice_candidates
                .lock()
                .expect("local ICE candidate collection lock must not be poisoned")
                .push(ServerMediaIceCandidateInit {
                    candidate: String::new(),
                    sdp_mid: None,
                    sdp_mline_index: None,
                    username_fragment: None,
                });
        }
    }
}
```

Define the public DTO in the same file:

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ServerMediaIceCandidateInit {
    pub candidate: String,
    pub sdp_mid: Option<String>,
    pub sdp_mline_index: Option<u16>,
    pub username_fragment: Option<String>,
}

impl From<RTCIceCandidateInit> for ServerMediaIceCandidateInit {
    fn from(candidate: RTCIceCandidateInit) -> Self {
        Self {
            candidate: candidate.candidate,
            sdp_mid: candidate.sdp_mid,
            sdp_mline_index: candidate.sdp_mline_index,
            username_fragment: candidate.username_fragment,
        }
    }
}

impl From<ServerMediaIceCandidateInit> for RTCIceCandidateInit {
    fn from(candidate: ServerMediaIceCandidateInit) -> Self {
        Self {
            candidate: candidate.candidate,
            sdp_mid: candidate.sdp_mid,
            sdp_mline_index: candidate.sdp_mline_index,
            username_fragment: candidate.username_fragment,
            url: None,
        }
    }
}
```

Update `WebRtcStack::create_peer_connection` to construct the candidate collection and pass it into both handler and handle:

```rust
let local_ice_candidates = Arc::new(Mutex::new(Vec::new()));
let handler = Arc::new(PeerConnectionHandler {
    local_ice_candidates: Arc::clone(&local_ice_candidates),
});
let peer_connection = PeerConnectionBuilder::new()
    .with_handler(handler)
    .with_udp_addrs(vec!["127.0.0.1:0".to_owned()])
    .build()
    .await
    .map_err(|source| WebRtcStackError::CreatePeerConnection {
        source: Box::new(source),
    })?;

Ok(WebRtcPeerConnectionHandle {
    _peer_connection: Arc::from(peer_connection),
    local_ice_candidates,
})
```

Add the collection field:

```rust
pub struct WebRtcPeerConnectionHandle {
    _peer_connection: Arc<dyn PeerConnection>,
    local_ice_candidates: Arc<Mutex<Vec<ServerMediaIceCandidateInit>>>,
}
```

Add methods:

```rust
pub async fn add_remote_ice_candidate(
    &self,
    candidate: ServerMediaIceCandidateInit,
) -> Result<(), WebRtcStackError> {
    self._peer_connection
        .add_ice_candidate(candidate.into())
        .await
        .map_err(|source| WebRtcStackError::AddIceCandidate {
            source: Box::new(source),
        })
}

pub fn local_ice_candidates(&self) -> Vec<ServerMediaIceCandidateInit> {
    self.local_ice_candidates
        .lock()
        .expect("local ICE candidate collection lock must not be poisoned")
        .clone()
}
```

Add the error variant:

```rust
#[error("failed to add WebRTC ICE candidate")]
AddIceCandidate {
    #[source]
    source: Box<dyn Error + Send + Sync>,
},
```

Export `ServerMediaIceCandidateInit` from `crates/lyre-webrtc/src/lib.rs`.

- [ ] **Step 4: Run focused stack tests**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" -p lyre-webrtc ice_candidate local_ice
```

Expected: all candidate stack tests pass.

---

### Task 2: Add Negotiator Candidate Add and Query

**Files:**

- Create: `crates/lyre-webrtc/src/negotiation_tests.rs`
- Modify: `crates/lyre-webrtc/src/negotiation.rs`
- Modify: `crates/lyre-webrtc/src/lib.rs`

- [ ] **Step 1: Move existing negotiator tests out of the production module**

Create `crates/lyre-webrtc/src/negotiation_tests.rs` and move the current `#[cfg(test)] mod tests` content from `crates/lyre-webrtc/src/negotiation.rs` into the new file as top-level tests.

The new file should start with:

```rust
use std::sync::Arc;

use lyre_core::{RoomId, UserId};

use crate::{
    ServerMediaNegotiator, ServerMediaSessionKey, ServerMediaSessionRegistry,
    ServerMediaSessionState, ServerMediaOffer, WebRtcStack,
};
```

Keep the existing helper functions and tests unchanged except for removing `super::` prefixes where needed. In `crates/lyre-webrtc/src/lib.rs`, add:

```rust
#[cfg(test)]
mod negotiation_tests;
```

Remove the old inline `#[cfg(test)] mod tests` block from `crates/lyre-webrtc/src/negotiation.rs`.

Change the existing test-only debug accessor in `crates/lyre-webrtc/src/negotiation.rs` so the sibling test module can keep the existing repeated-negotiation handle replacement assertion:

```rust
#[cfg(test)]
pub(crate) fn stored_peer_connection_debug_id(&self, key: &ServerMediaSessionKey) -> Option<usize> {
    self.peer_connections
        .get(key)
        .map(|entry| entry.value().debug_id())
}
```

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" -p lyre-webrtc negotiation
wc -l crates/lyre-webrtc/src/negotiation.rs crates/lyre-webrtc/src/negotiation_tests.rs
```

Expected: existing negotiation tests still pass and both files stay below 400 LOC.

- [ ] **Step 2: Write failing negotiator candidate tests**

Add `ServerMediaIceCandidate` and `ServerMediaNegotiationError` to the existing `use crate::{ ... }` list in `crates/lyre-webrtc/src/negotiation_tests.rs`, then add these helpers and tests:

```rust
use crate::{
    ServerMediaIceCandidate, ServerMediaNegotiationError, ServerMediaNegotiator,
    ServerMediaOffer, ServerMediaSessionKey, ServerMediaSessionRegistry,
    ServerMediaSessionState, WebRtcStack,
};
```

```rust
fn host_candidate() -> ServerMediaIceCandidate {
    ServerMediaIceCandidate {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
        candidate: "candidate:1 1 UDP 2130706431 192.168.1.100 54321 typ host".to_owned(),
        sdp_mid: Some("0".to_owned()),
        sdp_mline_index: Some(0),
        username_fragment: None,
    }
}

#[tokio::test]
async fn add_remote_ice_candidate_succeeds_for_existing_peer_without_state_change() {
    let sessions = Arc::new(ServerMediaSessionRegistry::new());
    let negotiator = ServerMediaNegotiator::new(WebRtcStack::new(), Arc::clone(&sessions));
    negotiator
        .answer_offer(offer("audio-main", offer_sdp().await))
        .await
        .unwrap();

    negotiator
        .add_remote_ice_candidate(host_candidate())
        .await
        .unwrap();

    assert_eq!(sessions.sessions()[0].state, ServerMediaSessionState::Negotiating);
}

#[tokio::test]
async fn add_remote_ice_candidate_missing_peer_returns_error_without_session() {
    let sessions = Arc::new(ServerMediaSessionRegistry::new());
    let negotiator = ServerMediaNegotiator::new(WebRtcStack::new(), Arc::clone(&sessions));

    let error = negotiator
        .add_remote_ice_candidate(host_candidate())
        .await
        .unwrap_err();

    assert!(matches!(error, ServerMediaNegotiationError::SessionMissing));
    assert!(sessions.sessions().is_empty());
}

#[tokio::test]
async fn local_ice_candidates_are_keyed_by_session() {
    let sessions = Arc::new(ServerMediaSessionRegistry::new());
    let negotiator = ServerMediaNegotiator::new(WebRtcStack::new(), Arc::clone(&sessions));
    let key = ServerMediaSessionKey {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
    };
    negotiator
        .answer_offer(offer("audio-main", offer_sdp().await))
        .await
        .unwrap();

    let candidates = wait_for_local_candidates(&negotiator, &key).await;

    assert!(candidates
        .iter()
        .any(|candidate| candidate.candidate.starts_with("candidate:")));
    assert!(candidates.iter().any(|candidate| candidate.candidate.is_empty()));
    assert!(candidates.iter().all(|candidate| candidate.room_id == key.room_id));
    assert!(candidates.iter().all(|candidate| candidate.user_id == key.user_id));
}
```

Add this test helper in the same module:

```rust
async fn wait_for_local_candidates(
    negotiator: &ServerMediaNegotiator,
    key: &ServerMediaSessionKey,
) -> Vec<ServerMediaIceCandidate> {
    for _ in 0..128 {
        let candidates = negotiator.local_ice_candidates(key);
        if candidates.iter().any(|candidate| candidate.candidate.starts_with("candidate:"))
            && candidates.iter().any(|candidate| candidate.candidate.is_empty())
        {
            return candidates;
        }
        tokio::task::yield_now().await;
    }
    negotiator.local_ice_candidates(key)
}
```

Update the existing `close_and_close_room_remove_stored_handles` test to assert that `local_ice_candidates` returns empty after `close` and after `close_room`.

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" -p lyre-webrtc negotiation_tests::add_remote_ice_candidate negotiation_tests::local_ice negotiation_tests::close_and_close_room
```

Expected: fails because negotiator candidate DTOs and methods do not exist.

- [ ] **Step 3: Implement negotiator candidate DTO and methods**

In `crates/lyre-webrtc/src/negotiation.rs`, import `ServerMediaIceCandidateInit` and define:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerMediaIceCandidate {
    pub room_id: RoomId,
    pub user_id: UserId,
    pub candidate: String,
    pub sdp_mid: Option<String>,
    pub sdp_mline_index: Option<u16>,
    pub username_fragment: Option<String>,
}

impl ServerMediaIceCandidate {
    fn key(&self) -> ServerMediaSessionKey {
        ServerMediaSessionKey {
            room_id: self.room_id.clone(),
            user_id: self.user_id.clone(),
        }
    }

    fn init(&self) -> ServerMediaIceCandidateInit {
        ServerMediaIceCandidateInit {
            candidate: self.candidate.clone(),
            sdp_mid: self.sdp_mid.clone(),
            sdp_mline_index: self.sdp_mline_index,
            username_fragment: self.username_fragment.clone(),
        }
    }

    fn from_init(key: &ServerMediaSessionKey, candidate: ServerMediaIceCandidateInit) -> Self {
        Self {
            room_id: key.room_id.clone(),
            user_id: key.user_id.clone(),
            candidate: candidate.candidate,
            sdp_mid: candidate.sdp_mid,
            sdp_mline_index: candidate.sdp_mline_index,
            username_fragment: candidate.username_fragment,
        }
    }
}
```

Add methods to `impl ServerMediaNegotiator`:

```rust
pub async fn add_remote_ice_candidate(
    &self,
    candidate: ServerMediaIceCandidate,
) -> Result<(), ServerMediaNegotiationError> {
    let key = candidate.key();
    let peer_connection = self
        .peer_connections
        .get(&key)
        .ok_or(ServerMediaNegotiationError::SessionMissing)?
        .clone();
    peer_connection
        .add_remote_ice_candidate(candidate.init())
        .await
        .map_err(|source| ServerMediaNegotiationError::WebRtc { source })?;
    Ok(())
}

pub fn local_ice_candidates(
    &self,
    key: &ServerMediaSessionKey,
) -> Vec<ServerMediaIceCandidate> {
    self.peer_connections
        .get(key)
        .map(|peer_connection| {
            peer_connection
                .local_ice_candidates()
                .into_iter()
                .map(|candidate| ServerMediaIceCandidate::from_init(key, candidate))
                .collect()
        })
        .unwrap_or_default()
}
```

Export `ServerMediaIceCandidate` from `crates/lyre-webrtc/src/lib.rs`.

- [ ] **Step 4: Run focused negotiator tests**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" -p lyre-webrtc ice_candidate local_ice close_and_close_room
wc -l crates/lyre-webrtc/src/negotiation.rs crates/lyre-webrtc/src/negotiation_tests.rs
```

Expected: all candidate negotiator and stack tests pass and both negotiator files remain below 400 LOC.

---

### Task 3: Move Server-Media HTTP Handlers and Add Candidate Routes

**Files:**

- Create: `crates/lyre-web/src/api_server_media.rs`
- Modify: `crates/lyre-web/src/api.rs`
- Modify: `crates/lyre-web/src/error.rs`
- Modify: `crates/lyre-web/src/lib.rs`
- Modify: `crates/lyre-web/src/api_server_media_tests.rs`

- [ ] **Step 1: Write failing REST candidate tests**

In `crates/lyre-web/src/api_server_media_tests.rs`, add:

```rust
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
        if candidates.iter().any(|candidate| candidate["candidate"].as_str().unwrap().starts_with("candidate:"))
            && candidates.iter().any(|candidate| candidate["candidate"] == "")
        {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(body.as_array().unwrap().iter().any(|candidate| {
        candidate["room_id"] == "DEFAULT"
            && candidate["user_id"] == "user_01"
            && candidate["candidate"].as_str().unwrap().starts_with("candidate:")
    }));
    assert!(body.as_array().unwrap().iter().any(|candidate| {
        candidate["room_id"] == "DEFAULT"
            && candidate["user_id"] == "user_01"
            && candidate["candidate"] == ""
    }));
}
```

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" -p lyre-web server_media_candidate
```

Expected: fails because candidate routes do not exist.

- [ ] **Step 2: Move server-media handlers into a module**

Create `crates/lyre-web/src/api_server_media.rs`:

```rust
use crate::{api::AppState, error::ApiError};
use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use lyre_core::{RoomId, UserId};
use lyre_webrtc::{
    ServerMediaIceCandidate, ServerMediaOffer, ServerMediaSessionKey,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ServerMediaOfferRequest {
    user_id: UserId,
    audio_track_id: String,
    sdp: String,
}

#[derive(Debug, Deserialize)]
struct ServerMediaCandidateRequest {
    user_id: UserId,
    candidate: String,
    sdp_mid: Option<String>,
    sdp_mline_index: Option<u16>,
    username_fragment: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ServerMediaCandidatesQuery {
    user_id: UserId,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/rooms/{room_id}/server-media/offer", post(answer_server_media_offer))
        .route(
            "/api/rooms/{room_id}/server-media/candidates",
            post(add_server_media_ice_candidate).get(server_media_ice_candidates),
        )
}

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

async fn add_server_media_ice_candidate(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(request): Json<ServerMediaCandidateRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    let candidate = ServerMediaIceCandidate {
        room_id,
        user_id: request.user_id,
        candidate: request.candidate,
        sdp_mid: request.sdp_mid,
        sdp_mline_index: request.sdp_mline_index,
        username_fragment: request.username_fragment,
    };
    state
        .add_server_media_ice_candidate(candidate.clone())
        .await?;
    Ok(Json(candidate))
}

async fn server_media_ice_candidates(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Query(query): Query<ServerMediaCandidatesQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    Ok(Json(state.server_media_ice_candidates(&ServerMediaSessionKey {
        room_id,
        user_id: query.user_id,
    })))
}
```

In `crates/lyre-web/src/lib.rs`, add:

```rust
pub mod api_server_media;
```

Remove `ServerMediaOfferRequest` and `answer_server_media_offer` from `api.rs`. In `router`, replace the server-media offer route with:

```rust
.merge(crate::api_server_media::router())
```

Keep `api.rs` below 400 LOC after this move.

- [ ] **Step 3: Add AppState candidate methods and error mapping**

In `crates/lyre-web/src/api.rs`, import `ServerMediaIceCandidate` and `ServerMediaSessionKey`. Add:

```rust
pub async fn add_server_media_ice_candidate(
    &self,
    candidate: ServerMediaIceCandidate,
) -> Result<(), ServerMediaNegotiationError> {
    self.server_media_negotiator
        .add_remote_ice_candidate(candidate)
        .await
}

pub fn server_media_ice_candidates(
    &self,
    key: &ServerMediaSessionKey,
) -> Vec<ServerMediaIceCandidate> {
    self.server_media_negotiator.local_ice_candidates(key)
}
```

In `crates/lyre-web/src/error.rs`, map `WebRtcStackError::AddIceCandidate` to `400 BAD_REQUEST` and keep `SessionMissing` as `500 INTERNAL_SERVER_ERROR`.

- [ ] **Step 4: Run focused web tests and LOC check**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" -p lyre-web server_media
wc -l crates/lyre-web/src/api.rs crates/lyre-web/src/api_server_media.rs
```

Expected: server media tests pass, both files are below 400 LOC.

---

### Task 4: Update RIDL and Frontend API Wrappers

**Files:**

- Modify: `proto/lyre.ridl`
- Modify: `frontend/src/lib/api.ts`
- Modify: `frontend/src/lib/api.test.ts`
- Modify: `frontend/src/lib/lyre.gen.ts`

- [ ] **Step 1: Add failing frontend API tests**

In `frontend/src/lib/api.test.ts`, import:

```ts
  addServerMediaIceCandidate,
  getServerMediaIceCandidates,
  serverMediaCandidatesUrl,
  type ServerMediaIceCandidate,
```

and generated type:

```ts
  type ServerMediaIceCandidate as WebrpcServerMediaIceCandidate,
```

Add contract objects:

```ts
const serverMediaCandidateFromRestShape: ServerMediaIceCandidate = {
  room_id: "DEFAULT",
  user_id: "user_a",
  candidate: "candidate:1",
  sdp_mid: "0",
  sdp_mline_index: 0,
  username_fragment: null
};
void serverMediaCandidateFromRestShape;

const generatedServerMediaCandidateContract: WebrpcServerMediaIceCandidate = {
  roomID: "DEFAULT",
  userID: "user_a",
  candidate: "candidate:1",
  sdpMid: "0",
  sdpMLineIndex: 0,
  usernameFragment: undefined
};
void generatedServerMediaCandidateContract;
```

Add tests:

```ts
it("builds encoded server media candidate urls", () => {
  expect(serverMediaCandidatesUrl("Team A")).toBe(
    "https://api.example.test/api/rooms/Team%20A/server-media/candidates"
  );
});

it("serializes server media ICE candidate request body", async () => {
  await addServerMediaIceCandidate("DEFAULT", {
    user_id: "user_a",
    candidate: "candidate:1",
    sdp_mid: "0",
    sdp_mline_index: 0,
    username_fragment: null
  });

  expect(fetch).toHaveBeenCalledWith("https://api.example.test/api/rooms/DEFAULT/server-media/candidates", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      user_id: "user_a",
      candidate: "candidate:1",
      sdp_mid: "0",
      sdp_mline_index: 0,
      username_fragment: null
    })
  });
});

it("fetches server media ICE candidates with encoded user id", async () => {
  await getServerMediaIceCandidates("DEFAULT", "user a");

  expect(fetch).toHaveBeenCalledWith(
    "https://api.example.test/api/rooms/DEFAULT/server-media/candidates?user_id=user+a"
  );
});
```

Run:

```bash
cd frontend && npm test -- --run src/lib/api.test.ts
```

Expected: fails because wrappers and generated types do not exist yet.

- [ ] **Step 2: Update RIDL and regenerate TypeScript**

In `proto/lyre.ridl`, add:

```text
struct ServerMediaIceCandidate
  - roomID: string
  - userID: string
  - candidate: string
  - sdpMid?: string
  - sdpMLineIndex?: uint32
  - usernameFragment?: string
```

Add service entries:

```text
  # Documents POST /api/rooms/{room_id}/server-media/candidates; REST fetch remains the runtime transport in this increment.
  - AddServerMediaIceCandidate(roomID: string, userID: string, candidate: string, sdpMid?: string, sdpMLineIndex?: uint32, usernameFragment?: string) => (accepted: ServerMediaIceCandidate)
  # Documents GET /api/rooms/{room_id}/server-media/candidates?user_id={userID}; REST fetch remains the runtime transport in this increment.
  - GetServerMediaIceCandidates(roomID: string, userID: string) => (candidates: []ServerMediaIceCandidate)
```

Run:

```bash
cd frontend && npm run generate:webrpc
```

Expected: `frontend/src/lib/lyre.gen.ts` includes `ServerMediaIceCandidate`.

- [ ] **Step 3: Implement frontend wrappers**

In `frontend/src/lib/api.ts`, import `ServerMediaIceCandidate as WebrpcServerMediaIceCandidate` and add:

```ts
export type ServerMediaIceCandidate = Omit<
  WebrpcServerMediaIceCandidate,
  "roomID" | "userID" | "sdpMid" | "sdpMLineIndex" | "usernameFragment"
> & {
  room_id: string;
  user_id: string;
  sdp_mid?: string | null;
  sdp_mline_index?: number | null;
  username_fragment?: string | null;
};

export function serverMediaCandidatesUrl(roomId: string): string {
  return `${roomUrl(roomId)}/server-media/candidates`;
}

export async function addServerMediaIceCandidate(
  roomId: string,
  candidate: Omit<ServerMediaIceCandidate, "room_id">
): Promise<ServerMediaIceCandidate> {
  const response = await fetch(serverMediaCandidatesUrl(roomId), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(candidate)
  });
  return response.json();
}

export async function getServerMediaIceCandidates(
  roomId: string,
  userId: string
): Promise<ServerMediaIceCandidate[]> {
  const query = new URLSearchParams({ user_id: userId });
  const response = await fetch(`${serverMediaCandidatesUrl(roomId)}?${query.toString()}`);
  return response.json();
}
```

- [ ] **Step 4: Run focused frontend tests**

Run:

```bash
cd frontend && npm test -- --run src/lib/api.test.ts
```

Expected: API tests pass.

---

### Task 5: Documentation, Review Prep, and Verification

**Files:**

- Modify after implementation review approval: `MEMORY.md`
- Modify after implementation review approval: `docs/roadmap.md`

- [ ] **Step 1: Run focused verification before implementation review**

Run:

```bash
cargo fmt --all --check
cargo nextest run --manifest-path "Cargo.toml" -p lyre-webrtc ice_candidate local_ice
cargo nextest run --manifest-path "Cargo.toml" -p lyre-web server_media
cd frontend && npm test -- --run src/lib/api.test.ts
```

Expected: all focused checks pass.

- [ ] **Step 2: Request independent implementation review**

Generate a patch including untracked files:

```bash
git diff --binary > /tmp/server-media-ice-candidates.patch
git ls-files --others --exclude-standard | while read -r f; do git diff --binary --no-index -- /dev/null "$f" >> /tmp/server-media-ice-candidates.patch || true; done
```

Dispatch an independent reviewer with:

- spec path: `docs/superpowers/specs/2026-06-15-server-media-ice-candidates-design.md`
- plan path: `docs/superpowers/plans/2026-06-15-server-media-ice-candidates.md`
- patch path: `/tmp/server-media-ice-candidates.patch`
- focused verification output

Required verdict:

```text
VERDICT: APPROVE | REVISE
SPEC_COVERAGE:
- ...
BLOCKERS:
- ...
REQUIRED_CHANGES:
- ...
```

Expected: reviewer returns `VERDICT: APPROVE` before documentation and full verification.

- [ ] **Step 3: Update memory and roadmap after implementation approval**

Append to `MEMORY.md`:

```md
## 2026-06-15 Server Media ICE Candidate Exchange

- Added server media ICE candidate add/query REST boundaries for negotiated server peer connections.
- Kept candidate conversion and direct `webrtc` ICE types isolated inside `lyre-webrtc`.
- Server media ICE exchange is still control-plane only; RTP/RTCP, Opus, RNNoise ingestion, and browser playback remain future work.
```

Add to `docs/roadmap.md` Completed:

```md
- Server media ICE candidate exchange boundary.
```

Keep the Next items for real media termination, RNNoise processing, and processed audio broadcast.

- [ ] **Step 4: Run full verification before commit**

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

- All commands pass.
- Direct `webrtc::` usage appears only in `crates/lyre-webrtc/src`.
- Public APIs do not expose direct `webrtc::` types.
- Rust files remain under the 400 LOC split threshold.

- [ ] **Step 5: Commit and push**

Use Lore commit format:

```text
Add server media ICE candidate exchange

Create the trickle ICE control-plane path for negotiated server peer connections without claiming media transport or audio processing.

Constraint: Direct external webrtc ICE types stay isolated to lyre-webrtc.
Rejected: Switching the room UI to server media transport in this increment | ICE exchange alone does not provide RTP/Opus processing or playback.
Confidence: high
Scope-risk: moderate
Directive: Keep server media candidate exchange as control-plane until RTP/RTCP, Opus, noise processing, and playback are implemented.
Tested: cargo fmt --all --check; cargo clippy --workspace --all-targets -- -D warnings; cargo nextest run --manifest-path Cargo.toml --workspace; cd frontend && npm run generate:webrpc && npm test -- --run && npm run typecheck && npm run lint && npm run build; git diff --check; static webrtc leak and LOC checks
Not-tested: DTLS-SRTP media termination, RTP/RTCP forwarding, Opus decode/encode, RNNoise ingestion from WebRTC tracks, browser playback of processed server audio
```

Run:

```bash
git push origin main
```

Expected: push succeeds. If push is blocked by credentials, network, remote rejection, or branch policy, report the local commit SHA and exact push error.
