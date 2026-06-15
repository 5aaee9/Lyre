# Room Access Token Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add anonymous room-scoped access tokens so clients must prove possession before mutating room, signalling, media relay, or server-media state.

**Architecture:** `lyre-core::RoomRegistry` issues and validates server-private bearer tokens tied to `(room_id, user_id)`. `lyre-web` extracts bearer tokens from REST headers or WebSocket query params and rejects protected routes before mutation. The frontend stores a room-scoped session object and passes the token explicitly to protected helpers.

**Tech Stack:** Rust 2021, Axum 0.8, DashMap, `rand` 0.9 CSPRNG, base64url, Next.js/React, TypeScript, Vitest, WebRPC TypeScript generation.

---

## File Structure

- Modify `Cargo.toml`: add workspace `rand = "0.9"` because access tokens require CSPRNG entropy.
- Modify `crates/lyre-core/Cargo.toml`: depend on workspace `rand`.
- Modify `crates/lyre-core/src/room.rs`: add `RoomAccessToken`, token generation, token validation, and server-private storage.
- Modify `crates/lyre-core/src/lib.rs`: export `RoomAccessToken`.
- Modify `crates/lyre-web/src/error.rs`: add `ApiError::Unauthorized`.
- Modify `crates/lyre-web/src/api.rs`: add bearer extraction, protected checks, WebSocket query validation, and redacted tracing setup.
- Modify `crates/lyre-web/src/api_server_media.rs`: enforce token validation on server-media routes.
- Modify existing backend tests in `crates/lyre-web/src/api_tests.rs`, `api_media_tests.rs`, `api_server_media_tests.rs`, and `api_server_media_close_tests.rs`.
- Modify `proto/lyre.ridl` and regenerate `frontend/src/lib/lyre.gen.ts`.
- Modify `frontend/src/lib/api.ts`: expose `access_token`, add explicit `accessToken` parameters, and set bearer headers.
- Modify `frontend/src/lib/signalling.ts`: add `accessToken` to WebSocket URL construction.
- Modify `frontend/src/lib/server-media-audio.ts`: accept `accessToken` and pass it to protected server-media helpers.
- Modify frontend tests in `frontend/src/lib/api.test.ts`, `frontend/src/lib/signalling.test.ts`, `frontend/src/lib/server-media-audio.test.ts`, `frontend/src/app/page.test.tsx`, and `frontend/src/app/room/[roomId]/room-client.test.tsx`.
- Modify `frontend/src/app/page.tsx`: store room-scoped session after join.
- Modify `frontend/src/app/room/[roomId]/room-client.tsx`: read/write `lyre.roomSession`, rejoin on room mismatch, and pass tokens.
- Modify `MEMORY.md` and `docs/roadmap.md` after implementation review approval.

## Task 1: Core Room Access Tokens

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/lyre-core/Cargo.toml`
- Modify: `crates/lyre-core/src/room.rs`
- Modify: `crates/lyre-core/src/lib.rs`

- [ ] **Step 1: Add failing core tests**

Add these tests to `crates/lyre-core/src/room.rs`:

```rust
#[test]
fn join_returns_distinct_access_tokens() {
    let registry = RoomRegistry::new();
    let room_id = RoomId::default_room();

    let first = registry.join(room_id.clone(), JoinRoomRequest::default());
    let second = registry.join(room_id.clone(), JoinRoomRequest::default());

    assert!(!first.access_token.as_str().is_empty());
    assert_ne!(first.access_token, second.access_token);
}

#[test]
fn access_token_validates_room_and_user_tuple() {
    let registry = RoomRegistry::new();
    let room_id = RoomId::default_room();
    let response = registry.join(room_id.clone(), JoinRoomRequest::default());

    assert!(registry
        .validate_access_token(&room_id, &response.user.id, &response.access_token)
        .is_ok());
    assert!(registry
        .validate_access_token(
            &RoomId::parse_boundary("OTHER").unwrap(),
            &response.user.id,
            &response.access_token,
        )
        .is_err());
    assert!(registry
        .validate_access_token(
            &room_id,
            &UserId::from_external("other_user"),
            &response.access_token,
        )
        .is_err());
    assert!(registry
        .validate_access_token(
            &room_id,
            &response.user.id,
            &RoomAccessToken::from_external("unknown"),
        )
        .is_err());
}

#[test]
fn leave_invalidates_access_token() {
    let registry = RoomRegistry::new();
    let room_id = RoomId::default_room();
    let response = registry.join(room_id.clone(), JoinRoomRequest::default());

    registry.leave(&room_id, &response.user.id);

    assert!(registry
        .validate_access_token(&room_id, &response.user.id, &response.access_token)
        .is_err());
}

#[test]
fn room_snapshot_does_not_serialize_access_token() {
    let registry = RoomRegistry::new();
    let room_id = RoomId::default_room();
    registry.join(room_id.clone(), JoinRoomRequest::default());

    let json = serde_json::to_value(registry.snapshot(room_id)).unwrap();

    assert!(json.to_string().contains("users"));
    assert!(!json.to_string().contains("access_token"));
}
```

- [ ] **Step 2: Run core tests and verify failure**

Run:

```bash
cargo test -p lyre-core room::tests::join_returns_distinct_access_tokens room::tests::access_token_validates_room_and_user_tuple room::tests::leave_invalidates_access_token room::tests::room_snapshot_does_not_serialize_access_token
```

Expected: compile failures for missing `RoomAccessToken`, `access_token`, and `validate_access_token`.

- [ ] **Step 3: Implement core token model**

Add `rand.workspace = true` to `crates/lyre-core/Cargo.toml` and `rand = "0.9"` to workspace dependencies in `Cargo.toml`.

In `crates/lyre-core/src/room.rs`, add:

```rust
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::{rngs::StdRng, Rng as _, SeedableRng};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RoomAccessToken(String);

impl RoomAccessToken {
    pub fn new() -> Self {
        let mut rng = StdRng::from_os_rng();
        let bytes: [u8; 32] = rng.random();
        Self(URL_SAFE_NO_PAD.encode(bytes))
    }

    pub fn from_external(input: impl Into<String>) -> Self {
        Self(input.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for RoomAccessToken {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RoomAccessError {
    #[error("room access token is invalid")]
    Invalid,
}
```

Extend `JoinRoomResponse`:

```rust
pub struct JoinRoomResponse {
    pub user: UserProfile,
    pub room: RoomSnapshot,
    pub access_token: RoomAccessToken,
}
```

Store tokens server-side without modifying `UserProfile`:

```rust
#[derive(Debug, Default)]
struct RoomState {
    users: DashMap<UserId, UserProfile>,
    access_tokens: DashMap<UserId, RoomAccessToken>,
}
```

In `join`, create and store the token before returning:

```rust
let access_token = RoomAccessToken::new();
room.users.insert(user.id.clone(), user.clone());
room.access_tokens.insert(user.id.clone(), access_token.clone());
```

Return `access_token` in `JoinRoomResponse`. In `leave`, remove `room.access_tokens.remove(user_id)`.

Add validation:

```rust
pub fn validate_access_token(
    &self,
    room_id: &RoomId,
    user_id: &UserId,
    token: &RoomAccessToken,
) -> Result<(), RoomAccessError> {
    let Some(room) = self.rooms.get(room_id) else {
        return Err(RoomAccessError::Invalid);
    };
    match room.access_tokens.get(user_id) {
        Some(stored) if stored.value() == token => Ok(()),
        _ => Err(RoomAccessError::Invalid),
    }
}
```

Add member-level validation for routes such as `start_media_relay` that do not carry a user id:

```rust
pub fn validate_any_access_token(
    &self,
    room_id: &RoomId,
    token: &RoomAccessToken,
) -> Result<(), RoomAccessError> {
    let Some(room) = self.rooms.get(room_id) else {
        return Err(RoomAccessError::Invalid);
    };
    if room
        .access_tokens
        .iter()
        .any(|entry| entry.value() == token)
    {
        Ok(())
    } else {
        Err(RoomAccessError::Invalid)
    }
}
```

Export `RoomAccessError` and `RoomAccessToken` from `crates/lyre-core/src/lib.rs`.

- [ ] **Step 4: Run core tests and formatting**

Run:

```bash
cargo fmt --all --check
cargo test -p lyre-core room::tests
```

Expected: all `room::tests` pass.

## Task 2: REST and WebSocket Access Checks

**Files:**
- Modify: `crates/lyre-web/src/error.rs`
- Modify: `crates/lyre-web/src/api.rs`
- Modify: `crates/lyre-web/src/api_tests.rs`
- Modify: `crates/lyre-web/src/api_media_tests.rs`

- [ ] **Step 1: Add failing backend access tests**

In `crates/lyre-web/src/api_tests.rs`, update `room_routes_join_snapshot_and_leave` to read `access_token` and send `Authorization: Bearer <token>` on leave. Add tests:

```rust
#[tokio::test]
async fn protected_room_leave_requires_bearer_token() {
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
    let body = body_json(join).await;
    let user_id = body["user"]["id"].as_str().unwrap();

    let response = app
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

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(body_json(response).await["error"], "room access token is invalid");
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

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(body_json(response).await["error"], "room access token is invalid");
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
                    .body(Body::from(format!(r#"{{"user_id":"{}"}}"#, default.user_id)))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(body_json(response).await["error"], "room access token is invalid");
    }
}
```

Add test-only helpers:

```rust
#[derive(Debug)]
struct JoinedForTest {
    user_id: String,
    access_token: String,
}

async fn join_for_test(app: axum::Router, nickname: &str) -> JoinedForTest {
    join_room_for_test(app, "DEFAULT", nickname).await
}

async fn join_room_for_test(app: axum::Router, room_id: &str, nickname: &str) -> JoinedForTest {
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
```

In `crates/lyre-web/src/api_media_tests.rs`, update mutating media relay route tests to join first and send bearer tokens. Add:

```rust
#[tokio::test]
async fn media_relay_status_remains_public() {
    let app = router(AppState::default());

    let response = app
        .oneshot(get("/api/rooms/DEFAULT/media-relay"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn media_relay_start_requires_bearer_token() {
    let app = router(AppState::default());

    let response = app
        .oneshot(post_json(
            "/api/rooms/DEFAULT/media-relay/start",
            r#"{"noise":{"provider":"off","intensity":0.5,"voice_activity_threshold":0.35}}"#,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
```

- [ ] **Step 2: Run targeted backend tests and verify failure**

Run:

```bash
cargo test -p lyre-web api_tests::protected_room_leave_requires_bearer_token api_tests::protected_room_leave_rejects_token_for_different_user api_media_tests::media_relay_start_requires_bearer_token
```

Expected: tests fail because routes still accept unauthenticated requests.

Also run:

```bash
cargo test -p lyre-web api_tests::protected_room_leave_rejects_malformed_unknown_and_room_mismatched_tokens
```

Expected: fails because all five failure modes are not rejected with the required JSON body yet.

- [ ] **Step 3: Implement `ApiError::Unauthorized` and REST auth helper**

In `crates/lyre-web/src/error.rs`, add:

```rust
Unauthorized,
```

and map it in `IntoResponse`:

```rust
Self::Unauthorized => (
    StatusCode::UNAUTHORIZED,
    "room access token is invalid".to_owned(),
),
```

In `crates/lyre-web/src/api.rs`, import `HeaderMap` and add:

```rust
fn bearer_token(headers: &HeaderMap) -> Result<lyre_core::RoomAccessToken, ApiError> {
    let Some(value) = headers.get(axum::http::header::AUTHORIZATION) else {
        return Err(ApiError::Unauthorized);
    };
    let value = value.to_str().map_err(|_| ApiError::Unauthorized)?;
    let Some(token) = value.strip_prefix("Bearer ") else {
        return Err(ApiError::Unauthorized);
    };
    if token.is_empty() {
        return Err(ApiError::Unauthorized);
    }
    Ok(lyre_core::RoomAccessToken::from_external(token))
}

fn authorize_room_user(
    state: &AppState,
    room_id: &RoomId,
    user_id: &lyre_core::UserId,
    headers: &HeaderMap,
) -> Result<(), ApiError> {
    let token = bearer_token(headers)?;
    state
        .registry
        .validate_access_token(room_id, user_id, &token)
        .map_err(|_| ApiError::Unauthorized)
}

fn authorize_room_member(
    state: &AppState,
    room_id: &RoomId,
    headers: &HeaderMap,
) -> Result<(), ApiError> {
    let token = bearer_token(headers)?;
    state
        .registry
        .validate_any_access_token(room_id, &token)
        .map_err(|_| ApiError::Unauthorized)
}
```

Add `headers: HeaderMap` extractors to protected handlers:

- `leave_room`: validate `request.user_id`.
- `start_media_relay`: validate any member.
- `stop_media_relay`: validate `request.user_id`.
- `register_media_track`: validate `request.user_id`.

- [ ] **Step 4: Implement WebSocket token validation and redacted tracing**

Extend `WsQuery`:

```rust
struct WsQuery {
    user_id: String,
    access_token: String,
}
```

Before `ws.on_upgrade`, validate:

```rust
let token = lyre_core::RoomAccessToken::from_external(query.access_token);
state
    .registry
    .validate_access_token(&room_id, &user_id, &token)
    .map_err(|_| ApiError::Unauthorized)?;
```

Replace `TraceLayer::new_for_http()` with a trace span helper that records method and a redacted path without query:

```rust
use tracing::Level;
use tower_http::trace::{DefaultOnResponse, TraceLayer};

fn make_request_span<B>(request: &axum::http::Request<B>) -> tracing::Span {
    tracing::span!(
        Level::INFO,
        "request",
        method = %request.method(),
        path = redacted_trace_path(request.uri()),
    )
}

fn redacted_trace_path(uri: &axum::http::Uri) -> &str {
    uri.path()
}
```

Use this helper in `router`:

```rust
.layer(
    TraceLayer::new_for_http()
        .make_span_with(make_request_span)
        .on_response(DefaultOnResponse::new()),
)
```

Add a static unit test for the exact helper used by request tracing:

```rust
#[test]
fn request_trace_path_redacts_query_tokens() {
    let uri: axum::http::Uri = "/api/rooms/DEFAULT/ws?user_id=user_01&access_token=secret".parse().unwrap();
    assert_eq!(redacted_trace_path(&uri), "/api/rooms/DEFAULT/ws");
}
```

Add an integration-style tracing test that installs a temporary subscriber, sends a WebSocket upgrade request with `access_token=secret-token`, and asserts captured logs do not contain `secret-token` or `access_token`. Use a small test writer type:

```rust
#[tokio::test]
async fn websocket_request_trace_does_not_log_access_token_query() {
    let logs = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
    let writer = CapturedWriter(logs.clone());
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
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let output = String::from_utf8(logs.lock().unwrap().clone()).unwrap();
    assert!(!output.contains("secret-token"));
    assert!(!output.contains("access_token"));
}

#[derive(Clone)]
struct CapturedWriter(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for CapturedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
```

- [ ] **Step 5: Run targeted backend tests**

Run:

```bash
cargo fmt --all --check
cargo test -p lyre-web api_tests api_media_tests
```

Expected: updated tests pass.

Also run the trace-specific test directly:

```bash
cargo test -p lyre-web api_tests::websocket_request_trace_does_not_log_access_token_query api_tests::request_trace_path_redacts_query_tokens
```

Expected: both pass and captured logs do not contain `access_token` or the token value.

## Task 3: Server-Media Access Checks

**Files:**
- Modify: `crates/lyre-web/src/api_server_media.rs`
- Modify: `crates/lyre-web/src/api_server_media_tests.rs`
- Modify: `crates/lyre-web/src/api_server_media_close_tests.rs`

- [ ] **Step 1: Add failing server-media auth tests**

Add or update helpers in server-media tests to join and return `(user_id, access_token)`. Mutating and candidate GET requests must send bearer tokens.

Add tests:

```rust
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
```

- [ ] **Step 2: Run targeted server-media tests and verify failure**

Run:

```bash
cargo test -p lyre-web api_server_media_tests::server_media_offer_requires_bearer_token api_server_media_tests::server_media_candidates_reject_token_for_different_user
```

Expected: tests fail before auth is implemented.

- [ ] **Step 3: Implement server-media auth**

In `crates/lyre-web/src/api_server_media.rs`, import `HeaderMap` and reuse auth helpers from `api.rs`. Make `authorize_room_user` `pub(crate)` in `api.rs`.

- `answer_server_media_offer`: validate `request.user_id`.
- `add_server_media_ice_candidate`: validate `request.user_id`.
- `server_media_ice_candidates`: validate `query.user_id`.
- `close_server_media_session`: validate `request.user_id`.

Update existing route tests to use joined users and bearer tokens rather than arbitrary `user_01` without membership.

- [ ] **Step 4: Run server-media test suite**

Run:

```bash
cargo fmt --all --check
cargo test -p lyre-web api_server_media_tests api_server_media_close_tests
```

Expected: all server-media API tests pass.

## Task 4: WebRPC and Frontend API Token Plumbing

**Files:**
- Modify: `proto/lyre.ridl`
- Regenerate: `frontend/src/lib/lyre.gen.ts`
- Modify: `frontend/src/lib/api.ts`
- Modify: `frontend/src/lib/signalling.ts`
- Modify: `frontend/src/lib/server-media-audio.ts`
- Modify: `frontend/src/lib/api.test.ts`
- Modify: `frontend/src/lib/signalling.test.ts`
- Modify: `frontend/src/lib/server-media-audio.test.ts`

- [ ] **Step 1: Update failing frontend tests first**

In `frontend/src/lib/api.test.ts`, update generated and adapted join response shapes:

```ts
const joinResponseFromGeneratedDerivedShape: JoinRoomResponse = {
  access_token: "token_a",
  user: {
    id: "user_a",
    nickname: "Ada",
    joined_at: "2026-06-14T00:00:00Z",
    noise: { provider: "off", intensity: 0.5, voice_activity_threshold: 0.35 }
  },
  room: { room_id: "DEFAULT", users: [] }
};

const generatedJoinRoomResponseContract: WebrpcJoinRoomResponse = {
  accessToken: "token_a",
  user: {
    id: "user_a",
    nickname: "Ada",
    joinedAt: "2026-06-14T00:00:00Z",
    noise: { provider: WebrpcNoiseProvider.OFF, intensity: 0.5, voiceActivityThreshold: 0.35 }
  },
  room: { roomID: "DEFAULT", users: [] }
};
```

Update protected helper expectations:

```ts
await leaveRoom("DEFAULT", "user_a", "token_a");
expect(fetch).toHaveBeenCalledWith("https://api.example.test/api/rooms/DEFAULT/leave", {
  method: "POST",
  headers: { "authorization": "Bearer token_a", "content-type": "application/json" },
  body: JSON.stringify({ user_id: "user_a" })
});
```

Repeat explicit token expectations for:

- `startMediaRelay("DEFAULT", noise, "token_a")`
- `stopMediaRelay("DEFAULT", "user_a", "token_a")`
- `registerMediaTrack("DEFAULT", "user_a", "audio-main", "audio", "token_a")`
- `answerServerMediaOffer("DEFAULT", "user_a", "audio-main", "v=0", "token_a")`
- `addServerMediaIceCandidate("DEFAULT", candidate, "token_a")`
- `getServerMediaIceCandidates("DEFAULT", "user_a", "token_a")`
- `closeServerMediaSession("DEFAULT", "user_a", "token_a")`

In `frontend/src/lib/signalling.test.ts`, update:

```ts
expect(roomSocketUrl("Team A", "user_a", "token a")).toBe(
  "wss://api.example.test/api/rooms/Team%20A/ws?user_id=user_a&access_token=token+a"
);
```

In `frontend/src/lib/server-media-audio.test.ts`, update `makeSession` to pass `accessToken: "token_a"` and update assertions:

```ts
function makeSession() {
  return new ServerMediaAudioSession({
    roomId: "DEFAULT",
    userId: "user_a",
    accessToken: "token_a",
    iceServers: [{ urls: ["stun:stun.example:3478"], username: null, credential: null }],
    stream,
    pollIntervalMs: 10
  });
}
```

Offer and candidate assertions must include the token:

```ts
expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledWith(
  "DEFAULT",
  "user_a",
  "audio-main",
  "local-offer",
  "token_a"
);
expect(apiMocks.getServerMediaIceCandidates).toHaveBeenCalledWith("DEFAULT", "user_a", "token_a");
expect(apiMocks.addServerMediaIceCandidate).toHaveBeenCalledWith(
  "DEFAULT",
  {
    user_id: "user_a",
    candidate: "candidate:local",
    sdp_mid: "0",
    sdp_mline_index: 0,
    username_fragment: "ufrag"
  },
  "token_a"
);
```

- [ ] **Step 2: Run frontend tests and verify failure**

Run:

```bash
cd frontend && npm test -- --run src/lib/api.test.ts src/lib/signalling.test.ts src/lib/server-media-audio.test.ts
```

Expected: compile/test failures because helper signatures and generated types are not updated.

- [ ] **Step 3: Update RIDL and regenerate TypeScript**

In `proto/lyre.ridl`, add `accessToken`:

```ridl
struct JoinRoomResponse
  - user: UserProfile
  - room: RoomSnapshot
  - accessToken: string
```

Update service return:

```ridl
- JoinRoom(roomID: string, nickname?: string, noise?: NoiseCancellationConfig) => (user: UserProfile, room: RoomSnapshot, accessToken: string)
```

Run:

```bash
cd frontend && npm run generate:webrpc
```

- [ ] **Step 4: Implement frontend API helpers**

In `frontend/src/lib/api.ts`, update:

```ts
export type JoinRoomResponse = Omit<WebrpcJoinRoomResponse, "user" | "room" | "accessToken"> & {
  access_token: string;
  user: UserProfile;
  room: RoomSnapshot;
};

function bearerHeaders(accessToken: string): Record<string, string> {
  return { authorization: `Bearer ${accessToken}` };
}
```

Protected helper signatures should accept `accessToken` and merge headers:

```ts
headers: { ...bearerHeaders(accessToken), "content-type": "application/json" }
```

For protected GET:

```ts
await fetch(`${serverMediaCandidatesUrl(roomId)}?${query.toString()}`, {
  headers: bearerHeaders(accessToken)
});
```

In `frontend/src/lib/signalling.ts`, change:

```ts
export function roomSocketUrl(roomId: string, userId: string, accessToken: string): string {
  const url = new URL(apiBaseUrl());
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  url.pathname = `/api/rooms/${encodeURIComponent(roomId)}/ws`;
  url.search = new URLSearchParams({ user_id: userId, access_token: accessToken }).toString();
  return url.toString();
}

export function createRoomSocket(roomId: string, userId: string, accessToken: string): WebSocket {
  return new WebSocket(roomSocketUrl(roomId, userId, accessToken));
}
```

In `frontend/src/lib/server-media-audio.ts`, extend the input type:

```ts
type ServerMediaAudioSessionInput = {
  roomId: string;
  userId: string;
  accessToken: string;
  audioTrackId?: string;
  iceServers: IceServerConfig[];
  stream: MediaStream;
  pollIntervalMs?: number;
  onError?: (message: string) => void;
};
```

Pass `this.input.accessToken` into all protected helper calls:

```ts
const answer = await answerServerMediaOffer(
  this.input.roomId,
  this.input.userId,
  this.audioTrackId,
  offer.sdp ?? "",
  this.input.accessToken
);
await addServerMediaIceCandidate(this.input.roomId, {
  user_id: this.input.userId,
  candidate: candidate.candidate ?? "",
  sdp_mid: candidate.sdpMid ?? null,
  sdp_mline_index: candidate.sdpMLineIndex ?? null,
  username_fragment: candidate.usernameFragment ?? null
}, this.input.accessToken);
const candidates = await getServerMediaIceCandidates(
  this.input.roomId,
  this.input.userId,
  this.input.accessToken
);
```

- [ ] **Step 5: Run frontend lib tests**

Run:

```bash
cd frontend && npm test -- --run src/lib/api.test.ts src/lib/signalling.test.ts src/lib/server-media-audio.test.ts
```

Expected: tests pass.

## Task 5: Frontend Room Session Flow

**Files:**
- Modify: `frontend/src/app/page.tsx`
- Modify: `frontend/src/app/page.test.tsx`
- Modify: `frontend/src/app/room/[roomId]/room-client.tsx`
- Modify: `frontend/src/app/room/[roomId]/room-client.test.tsx`

- [ ] **Step 1: Add failing room session tests**

In `frontend/src/app/page.test.tsx`, update mocked `joinRoom` response to include `access_token: "token_a"` and assert:

```ts
expect(JSON.parse(sessionStorage.getItem("lyre.roomSession") ?? "{}")).toMatchObject({
  roomId: "DEFAULT",
  accessToken: "token_a",
  user: { id: "user_a" }
});
```

In `frontend/src/app/room/[roomId]/room-client.test.tsx`, update the mocked `joinRoom` response to include `access_token: "token_a"` and assertions:

```ts
expect(apiMocks.startMediaRelay).toHaveBeenCalledWith("DEFAULT", noise, "token_a");
expect(apiMocks.registerMediaTrack).toHaveBeenCalledWith("DEFAULT", "user_a", "audio-main", "audio", "token_a");
expect(apiMocks.closeServerMediaSession).toHaveBeenCalledWith("DEFAULT", "user_a", "token_a");
expect(apiMocks.leaveRoom).toHaveBeenCalledWith("DEFAULT", "user_a", "token_a");
```

Add a test for room mismatch:

```ts
it("rejoins when stored room session belongs to another room", async () => {
  sessionStorage.setItem("lyre.roomSession", JSON.stringify({
    roomId: "OTHER",
    accessToken: "old_token",
    user: makeUser("old_user")
  }));

  render(<RoomClient roomId="DEFAULT" />);

  await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());
  expect(sessionStorage.getItem("lyre.roomSession")).toContain("token_a");
});
```

- [ ] **Step 2: Run targeted room tests and verify failure**

Run:

```bash
cd frontend && npm test -- --run src/app/page.test.tsx src/app/room/[roomId]/room-client.test.tsx
```

Expected: tests fail because the components still use `lyre.currentUser` and old helper signatures.

- [ ] **Step 3: Implement room-scoped session storage**

In `frontend/src/app/page.tsx`, replace `lyre.currentUser` writes with:

```ts
sessionStorage.setItem("lyre.roomSession", JSON.stringify({
  roomId: targetRoom,
  user: response.user,
  accessToken: response.access_token
}));
```

In `frontend/src/app/room/[roomId]/room-client.tsx`, add:

```ts
type RoomSession = {
  roomId: string;
  user: UserProfile;
  accessToken: string;
};

function readRoomSession(roomId: string): RoomSession | null {
  const stored = sessionStorage.getItem("lyre.roomSession");
  if (!stored) {
    return null;
  }
  try {
    const parsed = JSON.parse(stored) as Partial<RoomSession>;
    if (parsed.roomId !== roomId || !parsed.user || !parsed.accessToken) {
      return null;
    }
    return parsed as RoomSession;
  } catch {
    return null;
  }
}
```

Track `accessToken` in component state/ref:

```ts
const [accessToken, setAccessToken] = useState<string | null>(null);
const accessTokenRef = useRef<string | null>(null);
```

After join:

```ts
const session = { roomId, user: response.user, accessToken: response.access_token };
sessionStorage.setItem("lyre.roomSession", JSON.stringify(session));
```

Use `createRoomSocket(roomId, user.id, session.accessToken)`. Pass `accessToken` to all protected API calls. Remove `lyre.currentUser` cleanup and remove `lyre.roomSession` on explicit leave.

When constructing `ServerMediaAudioSession`, pass the token through:

```ts
const session = new ServerMediaAudioSession({
  roomId,
  userId: currentUser.id,
  accessToken,
  iceServers,
  stream,
  onError: setStatus
});
```

- [ ] **Step 4: Run room frontend tests**

Run:

```bash
cd frontend && npm test -- --run src/app/page.test.tsx src/app/room/[roomId]/room-client.test.tsx
```

Expected: tests pass.

## Task 6: Documentation, Whole Verification, Review, Commit

**Files:**
- Modify: `MEMORY.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Run all targeted checks before docs**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path Cargo.toml --workspace
cd frontend && npm test -- --run
cd frontend && npm run typecheck
cd frontend && npm run lint
cd frontend && npm run build
git diff --check
```

Expected: all commands exit 0.

- [ ] **Step 2: Request independent implementation review**

Dispatch a fresh reviewer with the approved spec, this plan, the git diff, and verification output. Required verdict:

```text
VERDICT: APPROVE | REVISE
SPEC_COVERAGE:
- [implemented requirement or missing requirement]
BLOCKERS:
- [blocking gap or "None"]
REQUIRED_CHANGES:
- [change or "None"]
```

If the reviewer returns `REVISE`, fix the blockers and repeat verification and review.

- [ ] **Step 3: Update memory and roadmap after approval**

In `MEMORY.md`, add a concise entry:

```markdown
## 2026-06-15 - Room Access Tokens

- `join` now returns a room-scoped opaque access token stored only in server-private room state.
- Mutating room, signalling, media relay, and server-media routes validate bearer tokens; public discovery routes remain unauthenticated.
- WebSocket signalling uses an `access_token` query parameter because browser WebSockets cannot set `Authorization`; request tracing records only redacted paths.
- TURN remains NAT traversal only. Server-side denoise still requires the server-media decode/process/broadcast path. Future client-side denoise should use Rust WASM.
```

In `docs/roadmap.md`, move authentication and room access control to completed work and keep these TODOs:

- Full DeepFilterNet neural inference/configuration.
- Optional client-side noise cancellation using Rust WASM.
- Persistent room/user/session state.
- Production observability and metrics.
- Generated WebRPC Rust server/runtime path.

- [ ] **Step 4: Final verification**

Run the full verification commands again after docs:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path Cargo.toml --workspace
cd frontend && npm test -- --run
cd frontend && npm run typecheck
cd frontend && npm run lint
cd frontend && npm run build
git diff --check
```

- [ ] **Step 5: Commit and push**

Review status:

```bash
git status --short
git diff --stat
```

Do not stage the pre-existing untracked SDD artifacts unless they are intentionally part of this increment. Stage only files changed for room access tokens, `MEMORY.md`, `docs/roadmap.md`, and this spec/plan.

Commit with Lore protocol:

```text
Constrain room mutation to joined anonymous sessions

Constraint: Browser WebSockets cannot set Authorization headers, so signalling carries an access_token query with redacted request tracing.
Rejected: Account-backed authentication | outside this anonymous-room increment.
Rejected: TURN-based denoise | TURN relays encrypted packets and cannot perform server-side audio processing.
Confidence: high
Scope-risk: broad
Directive: Keep access tokens out of UserProfile, RoomSnapshot, signalling payloads, logs, public responses, and error messages.
Tested: cargo fmt --all --check; cargo clippy --workspace --all-targets -- -D warnings; cargo nextest run --manifest-path Cargo.toml --workspace; cd frontend && npm test -- --run; cd frontend && npm run typecheck; cd frontend && npm run lint; cd frontend && npm run build; git diff --check
Not-tested: Browser-to-browser manual WebRTC session over real NAT.
```

Push the current branch:

```bash
git push
```
