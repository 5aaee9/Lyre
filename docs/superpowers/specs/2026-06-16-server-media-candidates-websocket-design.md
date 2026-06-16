# Server-Media ICE Candidates over Room WebSocket Design

## Context

Lyre already opens an authenticated room WebSocket at `/api/rooms/{room_id}/ws` for presence and signalling. Browser server-media negotiation still exchanges ICE candidates through `/api/rooms/{room_id}/server-media/candidates`: the frontend posts local candidates with REST and polls the same REST endpoint every second for server candidates.

This duplicates transport state and keeps a polling loop on the HTTP API even though the browser already has a live, authenticated room socket. The candidate exchange is scoped to the current room user, so it fits the existing WebSocket authentication boundary.

## Decision

Move browser runtime server-media ICE candidate exchange to the existing room WebSocket. Keep the REST and WebRPC server-media candidate APIs as compatibility and testing surfaces.

The WebSocket path will use explicit server-media signal payloads instead of overloading the existing mesh-style `ice-candidate` payload:

- `server-media-ice-candidate`: browser sends one local ICE candidate to the server-media negotiator.
- `server-media-ice-candidates-request`: browser asks the server for the current local server-media ICE candidates for this room user.
- `server-media-ice-candidates`: server responds to the same socket with the current local server-media ICE candidates.

The browser may continue using a timer to request candidates over WebSocket until the session is closed. This increment removes REST polling from browser runtime code, but it does not require a new server-side push subscription from the WebRTC negotiator.

## Signal Schema

All new payloads are members of the existing `SignalPayload` tagged enum and keep `SignalMessage` metadata unchanged:

```json
{
  "type": "server-media-ice-candidate",
  "room_id": "DEFAULT",
  "sender_id": "user_a",
  "recipient_id": "user_a",
  "payload": {
    "type": "server-media-ice-candidate",
    "candidate": "candidate:...",
    "sdp_mid": "0",
    "sdp_mline_index": 0,
    "username_fragment": "ufrag"
  }
}
```

```json
{
  "type": "server-media-ice-candidates-request",
  "room_id": "DEFAULT",
  "sender_id": "user_a",
  "recipient_id": "user_a",
  "payload": {
    "type": "server-media-ice-candidates-request"
  }
}
```

```json
{
  "type": "server-media-ice-candidates",
  "room_id": "DEFAULT",
  "sender_id": "user_a",
  "recipient_id": "user_a",
  "payload": {
    "type": "server-media-ice-candidates",
    "candidates": [
      {
        "room_id": "DEFAULT",
        "user_id": "user_a",
        "candidate": "candidate:...",
        "sdp_mid": "0",
        "sdp_mline_index": 0,
        "username_fragment": null
      }
    ]
  }
}
```

`sdp_mline_index` intentionally matches the existing server-media REST/WebRPC DTO shape. The existing mesh `ice-candidate` payload keeps its current `sdp_m_line_index` field.

## Server Behavior

The room WebSocket handler continues to authenticate through the existing query token before upgrade. After deserializing a `SignalMessage`, it still validates that message `room_id` matches the socket room and `sender_id` matches the socket user.

For `server-media-ice-candidate`:

- The server builds a `lyre_webrtc::ServerMediaIceCandidate` using the socket room and socket user, ignoring any room/user identity that is not part of the payload.
- The server calls `AppState::add_server_media_ice_candidate`.
- On success, the server does not broadcast or acknowledge the candidate.
- On failure, the server sends the existing `error` signal payload back to the same socket.
- Logs must not include raw ICE candidate strings; use the existing sanitized ICE candidate summary when logging candidate details.

For `server-media-ice-candidates-request`:

- The server calls `AppState::server_media_ice_candidates` with the socket room and socket user.
- The server sends a `server-media-ice-candidates` signal back to the same socket.
- The response is targeted to the same user and must not be broadcast to other room members.
- Logs must use sanitized candidate summaries.

For all other valid signal payloads, the existing peer forwarding behavior remains unchanged.

## Frontend Behavior

`ServerMediaAudioSession` receives the existing room WebSocket from `RoomClient`.

The session still uses REST for `/server-media/offer` and `/server-media/close` through the existing API client. Only ICE candidate exchange moves to WebSocket:

- Local ICE candidates are queued until the server-media answer is applied, then sent as `server-media-ice-candidate` WebSocket messages.
- Server candidate polling becomes a WebSocket request timer that sends `server-media-ice-candidates-request`.
- Incoming `server-media-ice-candidates` messages are dispatched from `RoomClient.socket.onmessage` to the active `ServerMediaAudioSession`.
- `ServerMediaAudioSession` deduplicates received candidates before calling `RTCPeerConnection.addIceCandidate`.
- If the WebSocket is not open when a local candidate or request should be sent, the session reports an audio connection error through its existing `onError` callback.

Presence reduction continues to ignore server-media payloads. Existing room snapshot, user joined, user left, mesh offer, mesh answer, mesh ICE, and error handling remain intact.

## Non-Goals

- Do not remove `/api/rooms/{room_id}/server-media/candidates`.
- Do not remove WebRPC `AddServerMediaIceCandidate` or `GetServerMediaIceCandidates`.
- Do not add a new WebRTC negotiator push subscription in this increment.
- Do not reintroduce peer mesh audio or peer-to-peer media fallback.
- Do not change server-media offer, close, relay, or media runtime semantics.

## Acceptance Criteria

- The Rust signalling schema serializes and deserializes the three new server-media payloads with the exact wire names above.
- The room WebSocket accepts a local server-media ICE candidate from the authenticated socket user and applies it to that user's server-media negotiator session.
- The room WebSocket responds to a candidates request with `server-media-ice-candidates` on the same socket and does not forward the request to peers.
- Invalid server-media candidate handling returns an `error` signal on the same socket without leaking raw candidate strings into logs.
- `ServerMediaAudioSession` no longer imports or calls `addServerMediaIceCandidate` or `getServerMediaIceCandidates`.
- Browser local ICE candidates and candidate requests are encoded as WebSocket `SignalMessage` objects using the current room id and user id.
- `RoomClient` passes the existing room socket to `ServerMediaAudioSession` and dispatches incoming server-media candidate responses to the active session.
- REST and WebRPC server-media candidate tests continue to pass unchanged except for documentation comments.
- `docs/roadmap.md` records that browser runtime server-media ICE candidate exchange now uses the room WebSocket while REST/WebRPC remain compatibility surfaces.

## Tests

Rust:

- Add focused signalling tests for the new payload wire names and field names.
- Add focused WebSocket tests in a new test module rather than growing `api_server_media_tests.rs`, which is already over the project line-count threshold.
- Cover candidate request response routing to the same socket.
- Cover local candidate submission through WebSocket for an existing server-media session.
- Cover missing server-media session error response for candidate submission.

Frontend:

- Update `server-media-audio.test.ts` to use a mock WebSocket and assert WebSocket candidate/request messages instead of REST add/get calls.
- Keep offer negotiation assertions through `answerServerMediaOffer`.
- Add or update a room client test to assert incoming `server-media-ice-candidates` WebSocket messages are handed to the active server-media audio session.
- Keep candidate deduplication coverage.

Verification:

- `cargo fmt`
- `cargo clippy --workspace --all-targets -- -D warnings`
- Targeted Rust tests for signalling, WebSocket server-media candidates, and existing REST/WebRPC server-media candidates.
- Targeted frontend tests for `server-media-audio`, room client socket dispatch, and signalling.
- `npm --prefix frontend run typecheck`
- `npm --prefix frontend run lint`
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`

## Documentation Impact

Update `docs/roadmap.md`. Update `proto/lyre.ridl` comments so REST/WebRPC candidate operations are described as compatibility surfaces, not the browser runtime transport.
