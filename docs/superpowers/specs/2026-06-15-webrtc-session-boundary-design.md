# WebRTC Session Boundary Design

## Scope

This increment introduces a dependency-isolated Rust WebRTC server session boundary for future media termination. It does not complete browser-to-server negotiation, DTLS-SRTP media termination, RTP/RTCP packet processing, Opus decode/encode, RNNoise frame ingestion, or client playback of processed server audio.

## Dependency Decision

Use the `webrtc` crate from the `webrtc-rs` project for the server WebRTC stack boundary. Current crate discovery shows `webrtc = "0.20.0-alpha.1"` as the high-level pure Rust WebRTC API, while `str0m = "0.20.0"` is a lower-level Sans-I/O alternative. This increment chooses `webrtc` because its verified `PeerConnectionBuilder`/`PeerConnection` model, RTP transceiver APIs, and `on_track` event model match Lyre's existing browser-style WebRTC signalling flow more directly. To contain alpha API churn, all direct `webrtc` imports stay inside a new `crates/lyre-webrtc` crate.

## Problem

Lyre now has decoded PCM processing, processed-frame broadcast, and egress fanout boundaries, but no Rust WebRTC media session boundary that future signalling code can use to terminate browser media. Adding complete SFU behavior is still too large for one increment. The next safe step is to isolate the WebRTC dependency behind Lyre-owned types and provide a tested session registry/control-plane boundary.

## Design

Add a new workspace crate:

- `crates/lyre-webrtc`

The crate owns:

- `ServerMediaSessionKey { room_id, user_id }`
- `ServerMediaSessionConfig { room_id, user_id, audio_track_id }`
- `ServerMediaSessionStatus { room_id, user_id, audio_track_id, state }`
- `ServerMediaSessionState` enum with `New`, `Negotiating`, `Connected`, `Closed`
- `ServerMediaSessionRegistry`, an in-memory session registry keyed by room/user
- `WebRtcStack`, a cloneable wrapper around the isolated `webrtc` API construction

The registry should:

- `start(config) -> ServerMediaSessionStatus`: create or replace a session for a room/user. Replacement resets the session to `New` and replaces `audio_track_id`.
- `sessions() -> Vec<ServerMediaSessionStatus>`: return deterministic status snapshots sorted by room id then user id, including `Closed` sessions.
- `active_sessions() -> Vec<ServerMediaSessionStatus>`: return deterministic snapshots excluding `Closed` sessions.
- `close(key) -> Option<ServerMediaSessionStatus>`: mark one existing session `Closed` and return its status, or return `None` when the session does not exist.
- `close_room(room_id) -> Vec<ServerMediaSessionStatus>`: mark all sessions for the room `Closed` and return the closed statuses sorted by room id then user id. It returns an empty vector when the room has no sessions.
- avoid exposing `webrtc` crate types in public APIs.

`WebRtcStack` must provide:

```rust
pub fn new() -> Self
pub async fn create_peer_connection(&self) -> Result<WebRtcPeerConnectionHandle, WebRtcStackError>
```

`WebRtcPeerConnectionHandle` and `WebRtcStackError` are Lyre-owned wrapper types, not re-exported `webrtc` types. `create_peer_connection` must internally build a real `webrtc::peer_connection::PeerConnectionBuilder` with a no-op event handler and loopback UDP address `"127.0.0.1:0"`, await `build()`, and store the resulting peer connection behind a private field. The handle does not expose SDP, ICE, tracks, or media methods yet; it exists to prove the dependency boundary compiles while keeping the alpha API isolated.

Wire `lyre-web::AppState` to own an `Arc<ServerMediaSessionRegistry>` and expose internal methods for future signalling work:

```rust
pub fn start_server_media_session(
    &self,
    config: ServerMediaSessionConfig,
) -> ServerMediaSessionStatus

pub fn server_media_sessions(&self) -> Vec<ServerMediaSessionStatus>

pub fn active_server_media_sessions(&self) -> Vec<ServerMediaSessionStatus>

pub fn close_server_media_sessions_for_room(
    &self,
    room_id: &RoomId,
) -> Vec<ServerMediaSessionStatus>
```

Stopping a media relay must also close server media sessions for that room.

## Acceptance Criteria

- A new `lyre-webrtc` crate exists in the workspace and is documented in `AGENTS.md`.
- The `webrtc` dependency is declared only in `lyre-webrtc`; `lyre-core` and `lyre-web` do not import it directly.
- No `lyre-webrtc` public API exposes concrete `webrtc` crate types in signatures or public fields. This must be verified by a static source check that allows `webrtc::` only in private implementation lines inside `crates/lyre-webrtc/src/stack.rs` and in no `pub` item signatures or public fields.
- `WebRtcStack::create_peer_connection` constructs a real `webrtc` peer connection internally and returns only a Lyre-owned handle or Lyre-owned error.
- The session registry can create/replace, list, active-list, close one, and close room sessions deterministically.
- Closed sessions remain visible in `sessions()` with `Closed` state and are excluded from `active_sessions()`.
- `lyre-web::AppState` owns the registry and uses the same room ids/user ids as the rest of the media relay state.
- Stopping a media relay closes server media sessions for that room.
- Public REST/WebSocket/frontend behavior remains unchanged.
- Documentation does not claim real WebRTC media termination, Opus/RTP forwarding, or processed audio playback.

## Tests

Add focused Rust tests covering:

- session create/replace/list/close behavior in `lyre-webrtc`,
- deterministic session status ordering,
- closed sessions remain in all-session snapshots and are excluded from active snapshots,
- `WebRtcStack::create_peer_connection` can be awaited without exposing `webrtc` types,
- no public `webrtc` types leaking through any `lyre-webrtc` public API signatures or public fields,
- AppState session registry sharing,
- media relay stop closes room sessions.

Run full verification after implementation:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`
- `cd frontend && npm run generate:webrpc && npm test -- --run && npm run typecheck && npm run lint && npm run build`
- `git diff --check`
- a static dependency leak check proving `webrtc::` does not appear outside `crates/lyre-webrtc/src/stack.rs` implementation internals and `Cargo.toml` dependency declarations

## Documentation

Update `AGENTS.md`, `MEMORY.md`, and `docs/roadmap.md`. The roadmap should mark the dependency-isolated WebRTC server session boundary complete while keeping real WebRTC media termination, RTP/RTCP, Opus decode/encode, RNNoise ingestion, and client playback as future work.

This increment must follow the repository's `$sdd-workflow`: independent spec review, independent plan review, implementation, independent implementation review, documentation update, fresh verification, Lore commit, and push.
