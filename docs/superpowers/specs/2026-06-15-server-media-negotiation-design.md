# Server Media Negotiation Boundary Design

## Scope

This increment adds the first browser-to-server WebRTC negotiation boundary for Lyre's future server media relay. It lets the Rust API accept a browser SDP offer for a room/user audio track, create a real server-side WebRTC answer through the isolated `lyre-webrtc` crate, and record the server media session as `Negotiating`.

This increment does not implement DTLS-SRTP media packet handling, RTP/RTCP routing, Opus decode/encode, RNNoise ingestion from a real WebRTC track, processed audio playback, trickle ICE candidate exchange, or frontend switching from the existing P2P mesh to server media relay.

## Problem

Lyre now has a dependency-isolated WebRTC crate and server media session registry, but there is no API path that performs browser-to-server offer/answer negotiation. Future media termination work needs a stable control-plane entry point that can own a server peer connection and return an answer without leaking `webrtc` crate types into `lyre-web` or `lyre-core`.

## Design

Add a Lyre-owned SDP DTO in `lyre-webrtc`:

- `ServerMediaOffer { room_id, user_id, audio_track_id, sdp }`
- `ServerMediaAnswer { room_id, user_id, audio_track_id, sdp, state }`

Extend `WebRtcPeerConnectionHandle` with a method that consumes a remote offer SDP string and returns a Lyre-owned answer:

```rust
pub async fn answer_remote_offer(
    &self,
    offer_sdp: String,
) -> Result<String, WebRtcStackError>
```

Internally this method must:

1. Wrap the SDP with `webrtc::peer_connection::sdp::session_description::RTCSessionDescription::offer`.
2. Call `set_remote_description`.
3. Call `create_answer(None)`.
4. Call `set_local_description`.
5. Return `local_description().await.sdp` when present.

The method must preserve the lower-level error as the `source` of `WebRtcStackError`.

Add a negotiation owner in `lyre-webrtc`:

- `ServerMediaNegotiator`
- `ServerMediaNegotiator::new(stack: WebRtcStack, sessions: Arc<ServerMediaSessionRegistry>) -> Self`
- `ServerMediaNegotiator::answer_offer(&self, offer: ServerMediaOffer) -> Result<ServerMediaAnswer, ServerMediaNegotiationError>`

The negotiator should:

1. Create a server peer connection through `WebRtcStack`.
2. Generate an answer using `WebRtcPeerConnectionHandle::answer_remote_offer`.
3. Only after answer generation succeeds, start or replace the session in `ServerMediaSessionRegistry` with the provided room/user/track config.
4. Mark the session state as `Negotiating`.
5. Store the peer connection handle in memory keyed by room/user so the connection is kept alive for later media work.
6. Return `ServerMediaAnswer` with the generated SDP and `Negotiating` state.

Negotiation is intentionally atomic from the registry/handle-map perspective:

- Invalid SDP must not create or replace a session.
- Peer connection creation failure must not create or replace a session.
- Answer generation failure must not create or replace a session.
- Failed negotiation must not insert or replace a stored peer connection handle.
- A repeated successful negotiation for the same room/user replaces the old session track id and old stored handle deterministically.
- Closing one session or closing a room must remove matching stored handles so closed sessions do not keep peer connections alive.

Extend `ServerMediaSessionRegistry` with the minimal state-transition method needed by negotiation:

```rust
pub fn set_state(
    &self,
    key: &ServerMediaSessionKey,
    state: ServerMediaSessionState,
) -> Option<ServerMediaSessionStatus>
```

Do not add generalized lifecycle machinery beyond this method.

Wire `lyre-web::AppState` to own an `Arc<ServerMediaNegotiator>` sharing the same `ServerMediaSessionRegistry`. Add an internal method:

```rust
pub async fn answer_server_media_offer(
    &self,
    offer: ServerMediaOffer,
) -> Result<ServerMediaAnswer, ServerMediaNegotiationError>
```

Add a REST endpoint:

```text
POST /api/rooms/{room_id}/server-media/offer
```

Request body:

```json
{
  "user_id": "user_01",
  "audio_track_id": "audio-main",
  "sdp": "v=0..."
}
```

Response body:

```json
{
  "room_id": "DEFAULT",
  "user_id": "user_01",
  "audio_track_id": "audio-main",
  "sdp": "v=0...",
  "state": "negotiating"
}
```

The route must parse the path room id at the boundary, use the path room id as authoritative, and reject invalid SDP or WebRTC negotiation failures through the existing `ApiError` JSON error path while preserving error context in the Rust error chain. The route does not require the media relay to be active yet; it prepares the WebRTC session boundary for future relay integration.

Update `proto/lyre.ridl` and `frontend/src/lib/api.ts` only enough to document and call the new REST endpoint. Do not wire the room UI to use this endpoint in this increment; the existing frontend P2P mesh remains the default runtime behavior.

## Acceptance Criteria

- `lyre-webrtc` can create a real WebRTC answer from a valid remote offer without exposing concrete `webrtc` crate types in public APIs.
- `ServerMediaNegotiator` stores server peer connection handles so negotiated sessions remain alive after the answer is returned.
- Negotiation starts or replaces the room/user session and marks it `Negotiating` after answer generation.
- Failed negotiation leaves the previous registry state and stored handle unchanged.
- Repeated successful negotiation for the same room/user replaces the previous stored handle and track id.
- Closing one session or closing a room removes matching stored handles from the negotiator.
- `ServerMediaSessionRegistry::set_state` updates only existing sessions and returns `None` for missing sessions.
- `lyre-web::AppState` shares one session registry between direct session methods and the negotiator.
- `POST /api/rooms/{room_id}/server-media/offer` returns an SDP answer and `negotiating` state for a valid offer.
- The route uses the path room id rather than trusting any room id in the body.
- Invalid SDP returns a client-visible error response through the existing API error shape.
- `proto/lyre.ridl` and frontend API wrappers include the new server media offer contract, but the room UI remains P2P mesh by default.
- Documentation does not claim RTP/RTCP forwarding, Opus decode/encode, RNNoise ingestion, or processed audio playback.

## Tests

Add focused Rust tests covering:

- `WebRtcPeerConnectionHandle::answer_remote_offer` can answer a valid offer produced by another `webrtc` peer connection.
- Invalid remote offer SDP preserves the underlying error through `WebRtcStackError`.
- `ServerMediaSessionRegistry::set_state` updates existing sessions and returns `None` for missing sessions.
- `ServerMediaNegotiator::answer_offer` returns a non-empty answer SDP, marks the session `Negotiating`, and keeps an active session status.
- Failed negotiation with invalid SDP does not create a session or insert a handle.
- Repeated successful negotiation for the same room/user replaces the track id and keeps only one stored handle.
- Closing one session and closing a room remove matching stored handles.
- `AppState` negotiator shares the same session registry as `AppState::server_media_sessions`.
- The REST route returns `200 OK` with `state = "negotiating"` and an answer SDP for a valid offer.
- The REST route returns a non-success response for invalid SDP.
- Frontend API tests cover the new URL and JSON request shape.

Run full verification after implementation:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`
- `cd frontend && npm run generate:webrpc && npm test -- --run && npm run typecheck && npm run lint && npm run build`
- `git diff --check`
- static dependency leak checks confirming `webrtc::` usage remains isolated to `crates/lyre-webrtc`

## Documentation

Update `MEMORY.md` and `docs/roadmap.md`. Record that server media offer/answer negotiation exists, while real media termination, RTP/RTCP handling, Opus decode/encode, RNNoise ingestion, and browser playback remain future work.

This increment must follow the repository's `$sdd-workflow`: independent spec review, independent plan review, implementation, independent implementation review, documentation update, fresh verification, Lore commit, and push.
