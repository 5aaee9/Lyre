# Server Media ICE Candidate Exchange Design

## Scope

This increment adds ICE candidate exchange for the server media WebRTC boundary. It lets the Rust API accept browser ICE candidates for an already negotiated server media peer connection and expose server-gathered ICE candidates through Lyre-owned DTOs.

This increment does not implement DTLS-SRTP media packet handling, RTP/RTCP routing, Opus decode/encode, RNNoise ingestion from WebRTC tracks, processed audio browser playback, or switching the room UI from the existing P2P mesh to server media relay.

## Problem

Lyre can now answer a browser SDP offer on the server, but the peer connection cannot receive trickled browser ICE candidates through the server media API. Future browser-to-server media transport needs a stable ICE control-plane path that keeps direct `webrtc` crate types isolated in `lyre-webrtc`.

## Design

Add a Lyre-owned ICE candidate DTO in `lyre-webrtc`:

```rust
pub struct ServerMediaIceCandidate {
    pub room_id: RoomId,
    pub user_id: UserId,
    pub candidate: String,
    pub sdp_mid: Option<String>,
    pub sdp_mline_index: Option<u16>,
    pub username_fragment: Option<String>,
}
```

Do not expose the external `RTCIceCandidateInit` type outside `lyre-webrtc`.

Extend `WebRtcPeerConnectionHandle` with:

```rust
pub async fn add_remote_ice_candidate(
    &self,
    candidate: ServerMediaIceCandidateInit,
) -> Result<(), WebRtcStackError>
```

where `ServerMediaIceCandidateInit` contains the WebRTC candidate fields but not room/user identity. The method converts to `RTCIceCandidateInit` internally and preserves lower-level errors as `WebRtcStackError::AddIceCandidate`.

Extend `ServerMediaNegotiator` with:

```rust
pub async fn add_remote_ice_candidate(
    &self,
    candidate: ServerMediaIceCandidate,
) -> Result<(), ServerMediaNegotiationError>
```

The negotiator should:

1. Look up the stored peer connection handle by room/user.
2. Return a typed `SessionMissing` error if there is no negotiated server media peer for the room/user.
3. Add the candidate to the stored peer connection.
4. Leave session registry state unchanged on success or failure.
5. Preserve lower-level WebRTC errors in the error chain.

Expose server-local ICE candidates with:

```rust
pub fn local_ice_candidates(&self, key: &ServerMediaSessionKey) -> Vec<ServerMediaIceCandidate>
```

`WebRtcStack::create_peer_connection` should install an event handler that records local candidates into the handle. This can be an in-memory, per-peer collection because the current API is stateless HTTP polling. The collection must remain behind Lyre-owned DTOs. End-of-candidates events with an empty candidate string should be retained so clients can observe completion.

Wire `lyre-web::AppState` with:

```rust
pub async fn add_server_media_ice_candidate(
    &self,
    candidate: ServerMediaIceCandidate,
) -> Result<(), ServerMediaNegotiationError>

pub fn server_media_ice_candidates(
    &self,
    key: &ServerMediaSessionKey,
) -> Vec<ServerMediaIceCandidate>
```

Add REST endpoints:

```text
POST /api/rooms/{room_id}/server-media/candidates
GET  /api/rooms/{room_id}/server-media/candidates?user_id=user_01
```

POST request body:

```json
{
  "user_id": "user_01",
  "candidate": "candidate:...",
  "sdp_mid": "0",
  "sdp_mline_index": 0,
  "username_fragment": "abc"
}
```

POST response body:

```json
{
  "room_id": "DEFAULT",
  "user_id": "user_01",
  "candidate": "candidate:...",
  "sdp_mid": "0",
  "sdp_mline_index": 0,
  "username_fragment": "abc"
}
```

GET response body:

```json
[
  {
    "room_id": "DEFAULT",
    "user_id": "user_01",
    "candidate": "candidate:...",
    "sdp_mid": "0",
    "sdp_mline_index": 0,
    "username_fragment": "abc"
  }
]
```

The route must parse the path room id at the boundary and use it as authoritative. A missing server media session should return a non-success API error through the existing `ApiError` shape. Invalid candidate strings or WebRTC candidate-add failures should preserve the lower-level error chain.

Update `proto/lyre.ridl` and `frontend/src/lib/api.ts` only enough to document and call the new REST endpoints. Do not wire the room UI to use this endpoint in this increment; the existing frontend P2P mesh remains the default runtime behavior.

## Acceptance Criteria

- `lyre-webrtc` can add a remote ICE candidate to an existing peer connection without exposing concrete `webrtc` crate types in public APIs.
- Adding a candidate to a missing room/user server media peer returns a typed missing-session error and does not create a session.
- Adding a candidate leaves the existing session status unchanged.
- WebRTC candidate-add failures preserve the lower-level error as the source error chain.
- Server-local ICE candidates are captured behind Lyre-owned DTOs and can be queried by room/user.
- Closing one session or closing a room removes the associated local candidate collection with the stored peer handle.
- `lyre-web::AppState` delegates candidate add/query through the shared `ServerMediaNegotiator`.
- `POST /api/rooms/{room_id}/server-media/candidates` uses the path room id, adds a candidate for an existing negotiated peer, and returns the accepted candidate DTO.
- POST to the candidate endpoint for a missing negotiated peer returns a non-success API response.
- `GET /api/rooms/{room_id}/server-media/candidates?user_id=...` returns server-local candidates for the negotiated peer.
- `proto/lyre.ridl` and frontend API wrappers include the new candidate exchange contract, but the room UI remains P2P mesh by default.
- Documentation does not claim RTP/RTCP forwarding, Opus decode/encode, RNNoise ingestion, or processed audio playback.

## Tests

Add focused Rust and frontend tests covering:

- `WebRtcPeerConnectionHandle::add_remote_ice_candidate` accepts a well-formed host candidate after remote description is set.
- Invalid remote ICE candidate input preserves the lower-level error through `WebRtcStackError`.
- `ServerMediaNegotiator::add_remote_ice_candidate` succeeds for an existing negotiated peer and keeps the session `Negotiating`.
- Missing server media peer candidate add returns `SessionMissing` without creating sessions.
- Local server candidates can be queried after answer generation.
- Closing one session or a room removes stored handles and associated local candidate query results.
- The REST POST candidate route returns `200 OK`, echoes the path room id, and ignores any body room id.
- The REST POST candidate route returns non-success for a missing negotiated peer.
- The REST GET candidate route returns a JSON list for a negotiated peer.
- Frontend API tests cover the new candidate URL, POST request body, GET query string, and generated WebRPC shape.

Run full verification after implementation:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`
- `cd frontend && npm run generate:webrpc && npm test -- --run && npm run typecheck && npm run lint && npm run build`
- `git diff --check`
- static dependency leak checks confirming direct `webrtc::` usage remains isolated to `crates/lyre-webrtc`

## Documentation

Update `MEMORY.md` and `docs/roadmap.md`. Record that server media ICE candidate exchange exists, while real media termination, RTP/RTCP handling, Opus decode/encode, RNNoise ingestion, and browser playback remain future work.

This increment must follow the repository's `$sdd-workflow`: independent spec review, independent plan review, implementation, independent implementation review, documentation update, fresh verification, Lore commit, and push.
