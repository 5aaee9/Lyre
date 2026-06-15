# WebRPC Rust Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Serve the checked-in WebRPC `Lyre` service from the Rust Axum API at `/rpc/Lyre/*`.

**Architecture:** Add a focused `lyre-web::webrpc` module that owns WebRPC DTOs, camelCase/uppercase enum conversion, error envelopes, and Axum handlers. The handlers reuse existing `AppState` methods so REST and WebRPC share persistence, metrics, room, media relay, and server-media behavior.

**Tech Stack:** Rust, Axum, Serde, existing `proto/lyre.ridl`, existing generated TypeScript client route/field conventions.

---

## Files

- Create: `crates/lyre-web/src/webrpc/mod.rs`
  - WebRPC router and module wiring.
- Create: `crates/lyre-web/src/webrpc/dto.rs`
  - WebRPC DTOs and conversions.
- Create: `crates/lyre-web/src/webrpc/error.rs`
  - WebRPC error envelope and sanitized `ApiError` mapping.
- Create: `crates/lyre-web/src/webrpc/handlers.rs`
  - WebRPC Axum handlers.
- Create: `crates/lyre-web/src/webrpc_tests.rs`
  - WebRPC endpoint tests and generated-client compatibility assertions.
- Modify: `crates/lyre-web/src/lib.rs`
  - Register `webrpc` and `webrpc_tests`.
- Modify: `crates/lyre-web/src/api.rs`
  - Merge `crate::webrpc::router()` into the existing API router and expose the existing authorization helper needed by the module.
- Modify: `README.md`, `MEMORY.md`, `docs/roadmap.md` after implementation review.

Keep each new runtime file focused and around the 400 LOC guideline where practical. If `handlers.rs` approaches the threshold, split room/media/server-media handlers into submodules before implementation review.

## Task 1: WebRPC DTOs and Conversions

**Files:**
- Create: `crates/lyre-web/src/webrpc/mod.rs`
- Create: `crates/lyre-web/src/webrpc/dto.rs`
- Modify: `crates/lyre-web/src/lib.rs`

- [ ] **Step 1: Register module**

In `crates/lyre-web/src/lib.rs`, add:

```rust
pub mod webrpc;

#[cfg(test)]
mod webrpc_tests;
```

- [ ] **Step 2: Create DTO skeleton**

Create `crates/lyre-web/src/webrpc/dto.rs` with request/response wrappers:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetRoomRequest {
    #[serde(rename = "roomID")]
    room_id: String,
}

#[derive(Debug, Serialize)]
struct GetRoomResponse {
    room: RoomSnapshot,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JoinRoomRequest {
    #[serde(rename = "roomID")]
    room_id: String,
    nickname: Option<String>,
    noise: Option<NoiseCancellationConfig>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct JoinRoomResponse {
    user: UserProfile,
    room: RoomSnapshot,
    access_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LeaveRoomRequest {
    #[serde(rename = "roomID")]
    room_id: String,
    #[serde(rename = "userID")]
    user_id: String,
}

#[derive(Debug, Serialize)]
struct LeaveRoomResponse {
    room: RoomSnapshot,
}
```

Add equivalent request/response structs for:

- `GetNoiseProvidersResponse { providers: Vec<NoiseCancellationConfig> }`
- `GetIceServersResponse { iceServers: Vec<IceServerConfig> }` using `#[serde(rename_all = "camelCase")]`
- `GetMediaTopologyResponse { topology: MediaTopology }`
- `GetMediaRelayRequest/Response`
- `StartMediaRelayRequest/Response`
- `StopMediaRelayRequest/Response`
- `RegisterMediaTrackRequest/Response`
- `AnswerServerMediaOfferRequest/Response`
- `AddServerMediaIceCandidateRequest/Response`
- `GetServerMediaIceCandidatesRequest/Response`
- `CloseServerMediaSessionRequest/Response`

Use explicit `#[serde(rename = "...")]` for RIDL acronym fields: `roomID`, `userID`, `trackID`, `audioTrackID`.

- [ ] **Step 3: Add WebRPC enum DTOs**

In `webrpc/dto.rs`, add WebRPC-facing enum DTOs:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum NoiseProvider {
    OFF,
    RNNOISE,
    DEEPFILTERNET,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum MediaTopologyMode {
    P2P_MESH,
    MEDIA_RELAY,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum MediaRelayStatus {
    INACTIVE,
    ACTIVE,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum MediaRelayMode {
    P2P_MESH,
    MEDIA_RELAY,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum MediaTrackKind {
    AUDIO,
    VIDEO,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum ServerMediaSessionState {
    NEW,
    NEGOTIATING,
    CONNECTED,
    CLOSED,
}
```

If clippy warns about variant casing, add a local allow for the WebRPC enum declarations because these names are the generated TypeScript contract.

- [ ] **Step 4: Add WebRPC value DTOs**

In `webrpc/dto.rs`, add DTOs matching RIDL fields:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NoiseCancellationConfig {
    provider: NoiseProvider,
    intensity: f32,
    voice_activity_threshold: f32,
}

#[derive(Debug, Clone, Serialize)]
struct RoomSnapshot {
    #[serde(rename = "roomID")]
    room_id: String,
    users: Vec<UserProfile>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UserProfile {
    id: String,
    nickname: String,
    joined_at: chrono::DateTime<chrono::Utc>,
    noise: NoiseCancellationConfig,
}
```

Add equivalent DTO structs for `IceServerConfig`, `MediaTopology`, `MediaRelayTrack`, `MediaRelayParticipant`, `MediaRelayRoomStatus`, `ServerMediaAnswer`, `ServerMediaSessionStatus`, `ServerMediaIceCandidate`, and `ClosedServerMediaSession`. Use `#[serde(rename_all = "camelCase")]` plus explicit `roomID/userID/trackID/audioTrackID` renames where needed.

- [ ] **Step 5: Add conversions**

In `webrpc/dto.rs`, implement `From` conversions from core/webrtc DTOs to WebRPC DTOs and `From` conversions from WebRPC request fragments to core request fragments:

```rust
impl From<lyre_core::NoiseProvider> for NoiseProvider { ... }
impl From<NoiseProvider> for lyre_core::NoiseProvider { ... }
impl From<lyre_core::NoiseCancellationConfig> for NoiseCancellationConfig { ... }
impl From<NoiseCancellationConfig> for lyre_core::NoiseCancellationConfig { ... }
impl From<lyre_core::RoomSnapshot> for RoomSnapshot { ... }
impl From<lyre_core::UserProfile> for UserProfile { ... }
impl From<lyre_core::MediaRelayRoomStatus> for MediaRelayRoomStatus { ... }
impl From<lyre_webrtc::ServerMediaAnswer> for ServerMediaAnswer { ... }
impl From<lyre_webrtc::ServerMediaIceCandidate> for ServerMediaIceCandidate { ... }
impl From<crate::api_server_media_state::CloseServerMediaSessionResponse> for ClosedServerMediaSession { ... }
```

Keep conversions exhaustive with `match`; do not use fallback/default enum branches.

- [ ] **Step 6: Add DTO unit tests**

At the bottom of `webrpc/dto.rs`, add small unit tests for serialization:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_config_serializes_with_webrpc_enum_and_camel_case_threshold() {
        let json = serde_json::to_value(NoiseCancellationConfig::from(
            lyre_core::NoiseCancellationConfig {
                provider: lyre_core::NoiseProvider::Rnnoise,
                intensity: 0.8,
                voice_activity_threshold: 0.2,
            },
        ))
        .unwrap();

        assert_eq!(json["provider"], "RNNOISE");
        assert_eq!(json["voiceActivityThreshold"], 0.2);
    }
}
```

- [ ] **Step 7: Run DTO compile/tests**

Run:

```bash
cargo test -p lyre-web webrpc::tests
```

Expected: WebRPC DTO unit tests pass.

## Task 2: WebRPC Error Envelope and Router

**Files:**
- Modify: `crates/lyre-web/src/webrpc/error.rs`
- Modify: `crates/lyre-web/src/webrpc/mod.rs`
- Modify: `crates/lyre-web/src/webrpc/handlers.rs`
- Modify: `crates/lyre-web/src/api.rs`

- [ ] **Step 1: Add WebRPC error type**

Create `crates/lyre-web/src/webrpc/error.rs` and add:

```rust
#[derive(Debug, Serialize)]
struct WebrpcErrorBody {
    name: &'static str,
    code: i32,
    message: String,
    status: u16,
}

#[derive(Debug)]
struct WebrpcError {
    status: StatusCode,
    message: String,
}

impl WebrpcError {
    fn bad_request() -> Self { ... }
    fn from_api(error: ApiError) -> Self { ... }
}

impl IntoResponse for WebrpcError {
    fn into_response(self) -> Response {
        let body = WebrpcErrorBody {
            name: "WebrpcEndpoint",
            code: 0,
            message: self.message,
            status: self.status.as_u16(),
        };
        (self.status, Json(body)).into_response()
    }
}
```

`from_api` must mirror status classes without leaking sensitive details:

- `BadRoomId`: status 400, message from validation.
- `MediaRelay`: status 409, message from relay error.
- `Persistence`: log `{error:#}` and return status 500/message `room state persistence failed`.
- `Unauthorized`: status 401/message `room access token is invalid`.
- `TurnRestCredentials`: status 500/message from non-secret TURN validation.
- `ServerMediaNegotiation`: log full error chain with context, return status 400 only for bad offer/add-candidate cases and 500 otherwise, message `server media negotiation failed`.

Do not include SDP, ICE candidate strings, access tokens, RTP payloads, or path details in WebRPC public errors.

- [ ] **Step 2: Add JSON extractor error handling**

Use Axum's default `Json<T>` extractor for handlers where possible. Add tests later for malformed JSON; if default extractor does not produce WebRPC envelope, switch route handlers to accept `Bytes` and deserialize with `serde_json::from_slice` through a helper:

```rust
fn parse_json<T: serde::de::DeserializeOwned>(bytes: axum::body::Bytes) -> Result<T, WebrpcError> {
    serde_json::from_slice(&bytes).map_err(|_| WebrpcError::bad_request())
}
```

The final implementation must satisfy malformed JSON test with WebRPC envelope.

- [ ] **Step 3: Add route skeleton**

Create `crates/lyre-web/src/webrpc/mod.rs` and add:

```rust
mod dto;
mod error;
mod handlers;

use axum::{routing::post, Router};
use crate::api::AppState;
use handlers::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/rpc/Lyre/GetRoom", post(get_room))
        .route("/rpc/Lyre/JoinRoom", post(join_room))
        .route("/rpc/Lyre/LeaveRoom", post(leave_room))
        .route("/rpc/Lyre/GetNoiseProviders", post(get_noise_providers))
        .route("/rpc/Lyre/GetIceServers", post(get_ice_servers))
        .route("/rpc/Lyre/GetMediaTopology", post(get_media_topology))
        .route("/rpc/Lyre/GetMediaRelay", post(get_media_relay))
        .route("/rpc/Lyre/StartMediaRelay", post(start_media_relay))
        .route("/rpc/Lyre/StopMediaRelay", post(stop_media_relay))
        .route("/rpc/Lyre/RegisterMediaTrack", post(register_media_track))
        .route("/rpc/Lyre/AnswerServerMediaOffer", post(answer_server_media_offer))
        .route("/rpc/Lyre/AddServerMediaIceCandidate", post(add_server_media_ice_candidate))
        .route("/rpc/Lyre/GetServerMediaIceCandidates", post(get_server_media_ice_candidates))
        .route("/rpc/Lyre/CloseServerMediaSession", post(close_server_media_session))
}
```

In `api.rs`, merge the router:

```rust
.merge(crate::webrpc::router())
```

- [ ] **Step 4: Expose authorization helper**

The `webrpc` handlers should reuse `authorize_room_user`. If needed, keep it `pub(crate)` so it is available inside the crate. Do not expose `authorize_room_member` beyond `pub(crate)` unless the WebRPC module needs member-level relay start.

If `StartMediaRelay` must use member-level authorization, expose a narrow `pub(crate) fn authorize_room_member(...)` with unchanged behavior.

- [ ] **Step 5: Implement simple public handlers**

Implement:

- `get_room`
- `join_room`
- `get_noise_providers`
- `get_ice_servers`
- `get_media_topology`
- `get_media_relay`

Each handler should return `Result<Json<ResponseDto>, WebrpcError>` and convert `ApiError` with `WebrpcError::from_api`.

Important logic:

- `join_room` calls `state.join_room_persisted(...)`, then `state.peers.user_joined(...)`, same as REST.
- `get_ice_servers` uses the same TURN credential generation path as REST.

- [ ] **Step 6: Run compile check**

Run:

```bash
cargo check -p lyre-web --all-targets
```

Expected: compile succeeds.

## Task 3: Protected Room and Media Relay WebRPC Handlers

**Files:**
- Modify: `crates/lyre-web/src/webrpc/handlers.rs`
- Modify: `crates/lyre-web/src/webrpc_tests.rs`

- [ ] **Step 1: Implement protected room/media handlers**

Implement:

- `leave_room`: parse room/user, authorize user, call `state.leave_room_persisted(...)`, call `state.peers.user_left(...)`, return `{ room }`.
- `start_media_relay`: parse room, authorize any room member, call `state.start_media_relay(...)`, return `{ mediaRelay }`.
- `stop_media_relay`: parse room/user, authorize user, call `state.stop_media_relay(...)`, return `{ mediaRelay }`.
- `register_media_track`: parse room/user/track/kind, authorize user, call `state.media_relays.register_track(...)`, return `{ mediaRelay }`.

- [ ] **Step 2: Add room/media WebRPC tests**

Create `crates/lyre-web/src/webrpc_tests.rs` with helpers:

```rust
use crate::api::{router, AppState};
use axum::{body::Body, http::{Request, StatusCode}};
use http_body_util::BodyExt;
use tower::ServiceExt;

async fn body_json(response: axum::response::Response) -> serde_json::Value { ... }
fn rpc_post(method: &str, body: serde_json::Value) -> Request<Body> { ... }
fn rpc_post_auth(method: &str, body: serde_json::Value, token: &str) -> Request<Body> { ... }
```

Add tests:

- `webrpc_join_get_and_leave_use_generated_client_shape`
- `webrpc_public_discovery_methods_return_wrapper_shapes`
- `webrpc_media_relay_methods_use_auth_and_wrapper_shapes`

Assertions must verify:

- `accessToken`, `room.roomID`, `user.joinedAt`, `noise.provider == "OFF"`.
- `GetRoom` response is `{ room: ... }`.
- `LeaveRoom` without auth returns status 401 with `name == "WebrpcEndpoint"`, not `{ error }`.
- Relay status fields use `mediaRelay.status == "ACTIVE"` / `"INACTIVE"`, `trackID`, `userID`, and `kind == "AUDIO"`.

- [ ] **Step 3: Run room/media WebRPC tests**

Run:

```bash
cargo test -p lyre-web webrpc_join_get_and_leave_use_generated_client_shape webrpc_public_discovery_methods_return_wrapper_shapes webrpc_media_relay_methods_use_auth_and_wrapper_shapes
```

If Cargo accepts only one filter, run `cargo test -p lyre-web webrpc_`.

Expected: tests pass.

## Task 4: Server-Media WebRPC Handlers

**Files:**
- Modify: `crates/lyre-web/src/webrpc/handlers.rs`
- Modify: `crates/lyre-web/src/webrpc_tests.rs`

- [ ] **Step 1: Implement server-media handlers**

Implement:

- `answer_server_media_offer`: authorize room/user, call `state.answer_server_media_offer(...)`, return `{ answer }`.
- `add_server_media_ice_candidate`: authorize room/user, call `state.add_server_media_ice_candidate(...)`, return `{ accepted }`.
- `get_server_media_ice_candidates`: authorize room/user, call `state.server_media_ice_candidates(...)`, return `{ candidates }`.
- `close_server_media_session`: authorize room/user, call `state.close_server_media_session_for_user(...)`, return `{ closed }`.

Use `lyre_webrtc::ServerMediaOffer`, `ServerMediaIceCandidate`, and `ServerMediaSessionKey` internally.

- [ ] **Step 2: Add successful server-media WebRPC tests**

In `webrpc_tests.rs`, add helper equivalent to existing REST tests:

```rust
async fn offer_sdp() -> String {
    let offerer = lyre_webrtc::WebRtcStack::new()
        .create_peer_connection()
        .await
        .unwrap();
    offerer.create_local_offer_for_test().await.unwrap()
}
```

Add test `webrpc_server_media_methods_return_wrapper_shapes`:

- Join through WebRPC and capture `userID` and `accessToken`.
- Start media relay through WebRPC with bearer token.
- Register an audio track through WebRPC.
- Call `AnswerServerMediaOffer` with a valid SDP and assert `answer.roomID`, `answer.userID`, `answer.audioTrackID`, `answer.state == "NEGOTIATING"`, and answer SDP starts with `v=0`.
- Call `AddServerMediaIceCandidate` with a valid host candidate and assert `{ accepted: { roomID, userID, candidate } }`.
- Poll `GetServerMediaIceCandidates` enough times to observe a wrapper `{ candidates: [...] }`; assert entries use `roomID`/`userID`.
- Call `CloseServerMediaSession` and assert `closed.mediaRelay.status == "ACTIVE"` and `closed.session.state == "CLOSED"`.

- [ ] **Step 3: Add sanitized error tests**

Add tests:

- `webrpc_server_media_methods_reject_missing_bearer_token`
- `webrpc_server_media_errors_do_not_echo_sdp_or_ice`
- `webrpc_malformed_json_returns_error_envelope`
- `webrpc_unknown_method_returns_plain_404`

For invalid SDP, send `sdp: "not sdp with secret-token candidate"` and assert:

- status is 400 or 500 per mapping
- body has `name == "WebrpcEndpoint"`
- body has no `error` field
- body string does not contain the invalid SDP text, `secret-token`, or candidate strings

- [ ] **Step 4: Run server-media WebRPC tests**

Run:

```bash
cargo test -p lyre-web webrpc_server_media
```

Expected: server-media WebRPC tests pass.

## Task 5: Documentation and Verification

**Files:**
- Modify: `README.md`
- Modify: `MEMORY.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Update README.md**

In the WebRPC section, replace the sentence that says runtime still uses REST only with:

```markdown
The Rust API also serves the generated client runtime at `/rpc/Lyre/*`; REST endpoints remain supported and are still used by the current frontend helper layer.
```

- [ ] **Step 2: Update MEMORY.md**

Add:

```markdown
## 2026-06-15 WebRPC Rust Runtime

- Added a repo-local Axum WebRPC runtime at `/rpc/Lyre/*` aligned with the checked-in RIDL and generated TypeScript client.
- Kept REST endpoints stable and shared `AppState` mutation paths so persistence, metrics, media relay, and server-media behavior stay consistent across REST and WebRPC.
- Used WebRPC-specific DTO conversion instead of changing core REST serde names; WebRPC responses use RIDL camelCase/acronym fields and uppercase enum values.
- Sanitized WebRPC public errors so SDP, ICE candidates, access tokens, media payloads, paths, and lower-level cause chains are not returned to clients.
```

- [ ] **Step 3: Update docs/roadmap.md**

Move `Integrate a generated WebRPC Rust server/runtime path.` from `Next` to `Completed`, phrased as:

```markdown
- RIDL-aligned Rust WebRPC runtime routes at `/rpc/Lyre/*` compatible with the generated TypeScript client.
```

- [ ] **Step 4: Run full verification**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
cd frontend && npm run generate:webrpc
cd frontend && npm test -- --run
cd frontend && npm run typecheck
cd frontend && npm run lint
cd frontend && npm run build
git diff --check
```

Expected: all commands pass. `npm run generate:webrpc` should not change `frontend/src/lib/lyre.gen.ts` unless the RIDL changed, which this increment does not require.
