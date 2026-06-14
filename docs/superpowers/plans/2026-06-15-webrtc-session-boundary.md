# WebRTC Session Boundary Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a dependency-isolated Rust WebRTC server session boundary for future media termination without claiming completed media transport.

**Architecture:** Add `crates/lyre-webrtc` to contain the direct `webrtc` dependency and expose only Lyre-owned session/control types. The new crate provides an in-memory `ServerMediaSessionRegistry` plus a minimal `WebRtcStack` wrapper that constructs a real `webrtc` peer connection internally. `lyre-web::AppState` owns the session registry and closes room sessions when the media relay stops, while public REST/WebSocket/frontend behavior remains unchanged.

**Tech Stack:** Rust workspace crate, `webrtc = "0.20.0-alpha.1"` isolated to `lyre-webrtc`, `dashmap`, existing `lyre-core` IDs, Axum app state.

---

### Task 1: Add Workspace Crate and Dependency Boundary

**Files:**

- Modify: `Cargo.toml`
- Create: `crates/lyre-webrtc/Cargo.toml`
- Create: `crates/lyre-webrtc/src/lib.rs`
- Create: `crates/lyre-webrtc/src/session.rs`
- Create: `crates/lyre-webrtc/src/stack.rs`

- [ ] **Step 1: Add workspace member and dependency**

In root `Cargo.toml`, add `crates/lyre-webrtc` to `workspace.members` and add workspace dependency:

```toml
webrtc = "0.20.0-alpha.1"
```

- [ ] **Step 2: Add crate manifest**

Create `crates/lyre-webrtc/Cargo.toml`:

```toml
[package]
name = "lyre-webrtc"
edition.workspace = true
license.workspace = true
version.workspace = true

[dependencies]
dashmap.workspace = true
lyre-core = { path = "../lyre-core" }
thiserror.workspace = true
webrtc.workspace = true

[dev-dependencies]
tokio.workspace = true
```

- [ ] **Step 3: Add crate module exports**

Create `crates/lyre-webrtc/src/lib.rs`:

```rust
pub mod session;
mod stack;

pub use session::{
    ServerMediaSessionConfig, ServerMediaSessionKey, ServerMediaSessionRegistry,
    ServerMediaSessionState, ServerMediaSessionStatus,
};
pub use stack::{WebRtcPeerConnectionHandle, WebRtcStack, WebRtcStackError};
```

- [ ] **Step 4: Add WebRtcStack boundary**

Create `crates/lyre-webrtc/src/stack.rs`:

```rust
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Clone, Default)]
pub struct WebRtcStack;

impl WebRtcStack {
    pub fn new() -> Self {
        Self
    }

    pub async fn create_peer_connection(
        &self,
    ) -> Result<WebRtcPeerConnectionHandle, WebRtcStackError> {
        let peer_connection = webrtc::peer_connection::PeerConnectionBuilder::new()
            .with_handler(Arc::new(NoopPeerConnectionHandler))
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .map_err(|source| WebRtcStackError::CreatePeerConnection {
                source: Box::new(source),
            })?;
        Ok(WebRtcPeerConnectionHandle {
            _peer_connection: Arc::new(peer_connection),
        })
    }
}

#[derive(Clone)]
pub struct WebRtcPeerConnectionHandle {
    _peer_connection: Arc<dyn webrtc::peer_connection::PeerConnection>,
}

impl std::fmt::Debug for WebRtcPeerConnectionHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WebRtcPeerConnectionHandle")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Error)]
pub enum WebRtcStackError {
    #[error("failed to create WebRTC peer connection")]
    CreatePeerConnection {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

#[derive(Debug)]
struct NoopPeerConnectionHandler;

impl webrtc::peer_connection::PeerConnectionEventHandler for NoopPeerConnectionHandler {}
```

This uses the verified `webrtc 0.20.0-alpha.1` API: `webrtc::peer_connection::PeerConnectionBuilder`, `PeerConnectionEventHandler`, and private `Arc<dyn webrtc::peer_connection::PeerConnection>` storage. Do not expose the concrete `webrtc` type publicly.

- [ ] **Step 5: Add stack test**

In `crates/lyre-webrtc/src/stack.rs`, add:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn creates_peer_connection_handle_without_exposing_webrtc_type() {
        let handle = WebRtcStack::new().create_peer_connection().await.unwrap();
        assert_eq!(
            std::any::type_name_of_val(&handle),
            "lyre_webrtc::stack::WebRtcPeerConnectionHandle"
        );
    }
}
```

This test proves the boundary can construct a real peer connection handle and that callers see the Lyre-owned handle type.

- [ ] **Step 6: Defer crate check**

Do not run `cargo check -p lyre-webrtc` yet because `lib.rs` intentionally re-exports session types that are implemented in Task 2. The first crate check runs after Task 2 creates `session.rs`.

### Task 2: Implement Session Registry

**Files:**

- Modify: `crates/lyre-webrtc/src/session.rs`

- [ ] **Step 1: Add session types and registry**

Create `crates/lyre-webrtc/src/session.rs`:

```rust
use dashmap::DashMap;
use lyre_core::{RoomId, UserId};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ServerMediaSessionKey {
    pub room_id: RoomId,
    pub user_id: UserId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerMediaSessionConfig {
    pub room_id: RoomId,
    pub user_id: UserId,
    pub audio_track_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerMediaSessionState {
    New,
    Negotiating,
    Connected,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerMediaSessionStatus {
    pub room_id: RoomId,
    pub user_id: UserId,
    pub audio_track_id: String,
    pub state: ServerMediaSessionState,
}

#[derive(Debug, Clone)]
struct ServerMediaSession {
    audio_track_id: String,
    state: ServerMediaSessionState,
}

#[derive(Debug, Default)]
pub struct ServerMediaSessionRegistry {
    sessions: DashMap<ServerMediaSessionKey, ServerMediaSession>,
}

impl ServerMediaSessionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&self, config: ServerMediaSessionConfig) -> ServerMediaSessionStatus {
        let key = ServerMediaSessionKey {
            room_id: config.room_id,
            user_id: config.user_id,
        };
        let session = ServerMediaSession {
            audio_track_id: config.audio_track_id,
            state: ServerMediaSessionState::New,
        };
        self.sessions.insert(key.clone(), session.clone());
        status_from_session(&key, &session)
    }

    pub fn sessions(&self) -> Vec<ServerMediaSessionStatus> {
        sorted_statuses(self.sessions.iter().map(|entry| {
            let key = entry.key().clone();
            let session = entry.value().clone();
            status_from_session(&key, &session)
        }))
    }

    pub fn active_sessions(&self) -> Vec<ServerMediaSessionStatus> {
        sorted_statuses(
            self.sessions
                .iter()
                .filter(|entry| entry.value().state != ServerMediaSessionState::Closed)
                .map(|entry| {
                    let key = entry.key().clone();
                    let session = entry.value().clone();
                    status_from_session(&key, &session)
                }),
        )
    }

    pub fn close(&self, key: &ServerMediaSessionKey) -> Option<ServerMediaSessionStatus> {
        let mut session = self.sessions.get_mut(key)?;
        session.state = ServerMediaSessionState::Closed;
        Some(status_from_session(key, &session))
    }

    pub fn close_room(&self, room_id: &RoomId) -> Vec<ServerMediaSessionStatus> {
        let statuses = self
            .sessions
            .iter_mut()
            .filter_map(|mut entry| {
                if &entry.key().room_id != room_id {
                    return None;
                }
                entry.value_mut().state = ServerMediaSessionState::Closed;
                Some(status_from_session(entry.key(), entry.value()))
            });
        sorted_statuses(statuses)
    }
}

fn status_from_session(
    key: &ServerMediaSessionKey,
    session: &ServerMediaSession,
) -> ServerMediaSessionStatus {
    ServerMediaSessionStatus {
        room_id: key.room_id.clone(),
        user_id: key.user_id.clone(),
        audio_track_id: session.audio_track_id.clone(),
        state: session.state,
    }
}

fn sorted_statuses(
    statuses: impl IntoIterator<Item = ServerMediaSessionStatus>,
) -> Vec<ServerMediaSessionStatus> {
    let mut statuses = statuses.into_iter().collect::<Vec<_>>();
    statuses.sort_by(|left, right| {
        left.room_id
            .cmp(&right.room_id)
            .then_with(|| left.user_id.cmp(&right.user_id))
    });
    statuses
}
```

- [ ] **Step 2: Add registry tests**

Add tests in the same file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn config(room: &str, user: &str, track: &str) -> ServerMediaSessionConfig {
        ServerMediaSessionConfig {
            room_id: RoomId::parse_boundary(room).unwrap(),
            user_id: UserId::from_external(user),
            audio_track_id: track.to_owned(),
        }
    }

    #[test]
    fn start_replaces_existing_session_and_resets_state() {
        let registry = ServerMediaSessionRegistry::new();
        let first = registry.start(config("DEFAULT", "user_01", "audio-main"));
        assert_eq!(first.state, ServerMediaSessionState::New);
        registry.close(&ServerMediaSessionKey {
            room_id: RoomId::default_room(),
            user_id: UserId::from_external("user_01"),
        });

        let replaced = registry.start(config("DEFAULT", "user_01", "audio-retry"));

        assert_eq!(replaced.audio_track_id, "audio-retry");
        assert_eq!(replaced.state, ServerMediaSessionState::New);
        assert_eq!(registry.sessions().len(), 1);
    }

    #[test]
    fn sessions_are_sorted_by_room_and_user() {
        let registry = ServerMediaSessionRegistry::new();
        registry.start(config("ROOM_B", "user_b", "audio-main"));
        registry.start(config("ROOM_A", "user_c", "audio-main"));
        registry.start(config("ROOM_A", "user_a", "audio-main"));

        let keys = registry
            .sessions()
            .into_iter()
            .map(|status| format!("{}:{}", status.room_id, status.user_id))
            .collect::<Vec<_>>();

        assert_eq!(keys, vec!["ROOM_A:user_a", "ROOM_A:user_c", "ROOM_B:user_b"]);
    }

    #[test]
    fn close_keeps_closed_session_in_all_sessions_only() {
        let registry = ServerMediaSessionRegistry::new();
        let key = ServerMediaSessionKey {
            room_id: RoomId::default_room(),
            user_id: UserId::from_external("user_01"),
        };
        registry.start(config("DEFAULT", "user_01", "audio-main"));

        let closed = registry.close(&key).unwrap();

        assert_eq!(closed.state, ServerMediaSessionState::Closed);
        assert_eq!(registry.sessions()[0].state, ServerMediaSessionState::Closed);
        assert!(registry.active_sessions().is_empty());
    }

    #[test]
    fn close_missing_session_returns_none() {
        let registry = ServerMediaSessionRegistry::new();

        assert_eq!(
            registry.close(&ServerMediaSessionKey {
                room_id: RoomId::default_room(),
                user_id: UserId::from_external("missing"),
            }),
            None
        );
    }

    #[test]
    fn close_room_closes_only_matching_room() {
        let registry = ServerMediaSessionRegistry::new();
        registry.start(config("DEFAULT", "user_01", "audio-main"));
        registry.start(config("OTHER", "user_02", "audio-main"));

        let closed = registry.close_room(&RoomId::default_room());

        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].state, ServerMediaSessionState::Closed);
        assert_eq!(registry.active_sessions()[0].room_id.as_str(), "OTHER");
    }
}
```

- [ ] **Step 3: Run registry tests**

Run:

```bash
cargo check -p lyre-webrtc
cargo nextest run --manifest-path "Cargo.toml" -p lyre-webrtc stack
cargo nextest run --manifest-path "Cargo.toml" -p lyre-webrtc session
```

Expected: crate compiles, the stack test passes, and session tests pass.

### Task 3: Wire lyre-web AppState

**Files:**

- Modify: `crates/lyre-web/Cargo.toml`
- Modify: `crates/lyre-web/src/api.rs`
- Modify: `crates/lyre-web/src/lib.rs` only if tests need a new test module
- Create: `crates/lyre-web/src/api_webrtc_session_tests.rs`

- [ ] **Step 1: Add lyre-web dependency**

In `crates/lyre-web/Cargo.toml`, add:

```toml
lyre-webrtc = { path = "../lyre-webrtc" }
```

- [ ] **Step 2: Add AppState field and methods**

In `crates/lyre-web/src/api.rs`, import:

```rust
use lyre_webrtc::{
    ServerMediaSessionConfig, ServerMediaSessionRegistry, ServerMediaSessionStatus,
};
```

Add field:

```rust
pub server_media_sessions: Arc<ServerMediaSessionRegistry>,
```

Initialize:

```rust
server_media_sessions: Arc::new(ServerMediaSessionRegistry::new()),
```

Add methods:

```rust
pub fn start_server_media_session(
    &self,
    config: ServerMediaSessionConfig,
) -> ServerMediaSessionStatus {
    self.server_media_sessions.start(config)
}

pub fn server_media_sessions(&self) -> Vec<ServerMediaSessionStatus> {
    self.server_media_sessions.sessions()
}

pub fn active_server_media_sessions(&self) -> Vec<ServerMediaSessionStatus> {
    self.server_media_sessions.active_sessions()
}

pub fn close_server_media_sessions_for_room(
    &self,
    room_id: &RoomId,
) -> Vec<ServerMediaSessionStatus> {
    self.server_media_sessions.close_room(room_id)
}
```

In `stop_media_relay`, after clearing processed media, call `state.close_server_media_sessions_for_room(&room_id);`.

- [ ] **Step 3: Add web tests**

Create `crates/lyre-web/src/api_webrtc_session_tests.rs`:

```rust
use crate::api::{router, AppState};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use lyre_core::{RoomId, StopMediaRelayRequest};
use lyre_webrtc::{ServerMediaSessionConfig, ServerMediaSessionState};
use tower::ServiceExt;

fn config(room_id: RoomId, user: &str, track: &str) -> ServerMediaSessionConfig {
    ServerMediaSessionConfig {
        room_id,
        user_id: lyre_core::UserId::from_external(user),
        audio_track_id: track.to_owned(),
    }
}

#[test]
fn app_state_owns_server_media_session_registry() {
    let state = AppState::default();
    let room_id = RoomId::default_room();

    state.start_server_media_session(config(room_id, "user_01", "audio-main"));

    assert_eq!(state.server_media_sessions().len(), 1);
    assert_eq!(state.active_server_media_sessions().len(), 1);
}

#[tokio::test]
async fn stopping_media_relay_closes_server_media_sessions_for_room() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    state
        .media_relays
        .start(room_id.clone(), lyre_core::StartMediaRelayRequest::default());
    state.start_server_media_session(config(room_id.clone(), "user_01", "audio-main"));
    let app = router(state.clone());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rooms/DEFAULT/media-relay/stop")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"user_id":"user_01"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        state.server_media_sessions()[0].state,
        ServerMediaSessionState::Closed
    );
    assert!(state.active_server_media_sessions().is_empty());
}
```

Remove unused imports after implementation.

- [ ] **Step 4: Register web test module**

In `crates/lyre-web/src/lib.rs`, add:

```rust
#[cfg(test)]
mod api_webrtc_session_tests;
```

- [ ] **Step 5: Run targeted web tests**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" -p lyre-web webrtc_session
```

Expected: tests pass.

### Task 4: Add Dependency Isolation Checks

**Files:**

- Verify source; no code change expected unless checks fail.

- [ ] **Step 1: Run static public API leak check**

Run:

```bash
rg -n "webrtc::" crates/lyre-core crates/lyre-web crates/lyre-webrtc/src Cargo.toml crates/*/Cargo.toml
```

Expected:

- root `Cargo.toml` contains `webrtc = "0.20.0-alpha.1"`,
- `crates/lyre-webrtc/Cargo.toml` contains `webrtc.workspace = true`,
- `crates/lyre-webrtc/src/stack.rs` has private implementation references to `webrtc::`,
- no `crates/lyre-core` or `crates/lyre-web` source references `webrtc::`,
- no `pub fn`, `pub struct` public field, or `pub use` exposes a `webrtc::` type.

Also run:

```bash
rg -n "pub (use|type) .*webrtc|pub [^\\n]*: .*webrtc::|pub (async )?fn [^\\n]*webrtc::" crates/lyre-webrtc/src
```

Expected: no matches. This catches direct public re-exports, public type aliases, public fields, and public function signatures that expose concrete `webrtc` types without rejecting Lyre-owned wrapper names like `WebRtcPeerConnectionHandle`.

- [ ] **Step 2: Run crate checks**

Run:

```bash
cargo check -p lyre-webrtc
cargo nextest run --manifest-path "Cargo.toml" -p lyre-webrtc
```

Expected: both pass.

### Task 5: Independent Implementation Review Gate

**Files:**

- Review the implemented diff before documentation and final verification.

- [ ] **Step 1: Capture implementation diff**

Run:

```bash
git diff -- Cargo.toml crates/lyre-webrtc crates/lyre-web/Cargo.toml crates/lyre-web/src/api.rs crates/lyre-web/src/api_webrtc_session_tests.rs crates/lyre-web/src/lib.rs > /tmp/webrtc-session-boundary.diff
```

- [ ] **Step 2: Dispatch independent implementation reviewer**

Send approved spec, reviewed plan, implementation diff, targeted verification output, and static leak-check output to a reviewer. Require:

```text
VERDICT: APPROVE | REVISE
SPEC_COVERAGE:
- [implemented requirement or missing requirement]
BLOCKERS:
- [blocking gap or "None"]
REQUIRED_CHANGES:
- [change or "None"]
```

Proceed to documentation only after `VERDICT: APPROVE`.

### Task 6: Update Documentation

**Files:**

- Modify: `AGENTS.md`
- Modify: `MEMORY.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Update AGENTS.md**

Add `crates/lyre-webrtc` to Workspace Crates and Key Dependencies. Wording:

```markdown
crates/lyre-webrtc - dependency-isolated Rust WebRTC server session boundary around the `webrtc` crate. Direct `webrtc` imports belong only in this crate until the media termination design is complete.
```

and:

```markdown
- **Server WebRTC boundary**: `lyre-webrtc` isolates the `webrtc` crate (`webrtc-rs`) behind Lyre-owned session/control types. Do not import `webrtc` directly from `lyre-core` or `lyre-web`.
```

- [ ] **Step 2: Update MEMORY.md**

Append:

```markdown
## 2026-06-15 WebRTC Session Boundary

- Added `lyre-webrtc` to isolate the direct `webrtc` crate dependency behind Lyre-owned server media session types.
- Chose `webrtc = "0.20.0-alpha.1"` over `str0m` for this boundary because its high-level API model better matches the existing browser-style signalling path.
- Server media sessions are control-plane state only; real browser-to-server negotiation, RTP/RTCP, Opus decode/encode, RNNoise ingestion, and playback remain future work.
```

- [ ] **Step 3: Update roadmap**

Add to Completed:

```markdown
- Dependency-isolated Rust WebRTC server session boundary in `lyre-webrtc`.
```

Keep real WebRTC media termination and client playback in Next.

### Task 7: Final Verification

**Files:**

- Verify the whole workspace.

- [ ] **Step 1: Check file sizes**

Run:

```bash
wc -l crates/lyre-webrtc/src/*.rs crates/lyre-web/src/api.rs crates/lyre-web/src/api_webrtc_session_tests.rs crates/lyre-web/src/lib.rs AGENTS.md
```

Expected: no touched Rust file exceeds 400 lines.

- [ ] **Step 2: Run Rust formatting check**

Run:

```bash
cargo fmt --all --check
```

Expected: exit 0.

- [ ] **Step 3: Run Rust clippy**

Run:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: exit 0.

- [ ] **Step 4: Run Rust tests**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: exit 0.

- [ ] **Step 5: Run frontend verification**

Run:

```bash
cd frontend && npm run generate:webrpc && npm test -- --run && npm run typecheck && npm run lint && npm run build
```

Expected: exit 0.

- [ ] **Step 6: Run dependency leak check**

Run:

```bash
rg -n "webrtc::" crates/lyre-core crates/lyre-web crates/lyre-webrtc/src Cargo.toml crates/*/Cargo.toml
```

Expected: only allowed dependency declarations and private `crates/lyre-webrtc/src/stack.rs` implementation references.

Run:

```bash
rg -n "pub (use|type) .*webrtc|pub [^\\n]*: .*webrtc::|pub (async )?fn [^\\n]*webrtc::" crates/lyre-webrtc/src
```

Expected: no matches.

- [ ] **Step 7: Check whitespace**

Run:

```bash
git diff --check
```

Expected: exit 0.

### Task 8: Commit and Push

**Files:**

- Commit all intended changes after verification.

- [ ] **Step 1: Review staged content**

Run:

```bash
git status --short
git diff --stat
```

Expected: only intended WebRTC session boundary, documentation, and plan/spec files are changed.

- [ ] **Step 2: Stage intended files**

Run:

```bash
git add Cargo.toml Cargo.lock AGENTS.md MEMORY.md docs/roadmap.md docs/superpowers/specs/2026-06-15-webrtc-session-boundary-design.md docs/superpowers/plans/2026-06-15-webrtc-session-boundary.md crates/lyre-webrtc crates/lyre-web/Cargo.toml crates/lyre-web/src/api.rs crates/lyre-web/src/api_webrtc_session_tests.rs crates/lyre-web/src/lib.rs
```

- [ ] **Step 3: Commit with Lore protocol**

Use a commit message with trailers:

```text
Isolate server WebRTC session boundary

Add a Lyre-owned server media session boundary around webrtc-rs so future media termination can be implemented without leaking alpha WebRTC APIs through core or web surfaces.

Constraint: The webrtc 0.20.0-alpha.1 dependency is isolated to lyre-webrtc.
Rejected: Direct webrtc imports in lyre-web | would spread alpha API churn into the HTTP/signalling layer.
Confidence: high
Scope-risk: moderate
Directive: Do not claim real media termination until browser negotiation, RTP/RTCP, Opus decode/encode, RNNoise ingestion, and client playback are implemented and verified.
Tested: cargo fmt --all --check; cargo clippy --workspace --all-targets -- -D warnings; cargo nextest run --manifest-path Cargo.toml --workspace; frontend generate:webrpc/test/typecheck/lint/build; dependency leak checks; git diff --check
Not-tested: Real WebRTC negotiation, DTLS-SRTP media termination, RTP/RTCP forwarding, Opus decode/encode, RNNoise ingestion, client playback
```

- [ ] **Step 4: Push**

Run:

```bash
git push
```

Expected: push succeeds. If it fails due to network/credentials/remote rejection, report the local commit SHA and exact push error.
