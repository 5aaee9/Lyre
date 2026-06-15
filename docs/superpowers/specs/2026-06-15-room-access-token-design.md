# Room Access Token Design

## Scope

This increment adds a minimal room-scoped access boundary for the existing anonymous VOIP flow. It does not add accounts, passwords, persistence, roles, room passwords, or database-backed authorization.

Joining a room remains public. A successful join returns an opaque access token bound to the returned `room_id` and `user.id`. Any API or WebSocket path that mutates room membership, signalling, media relay state, or server-media WebRTC state must prove possession of that token.

## Goals

- Prevent a client that only knows another user's `user_id` from leaving that user, registering tracks as that user, negotiating server media as that user, or connecting to signalling as that user.
- Preserve the shareable frontend room route `/room/[roomId]`.
- Keep unauthenticated discovery endpoints available so a new browser can load the room UI and then join.
- Keep the implementation in memory only, matching the current in-memory room registry.

## Non-Goals

- No durable sessions across API restarts.
- No user identity beyond the anonymous `UserProfile` created by `join`.
- No room ownership, moderator actions, invite links, or revocation UI.
- No TURN server implementation in this increment.
- No client-side noise cancellation implementation in this increment.

## Architecture

`lyre-core` owns token creation and validation inside `RoomRegistry` because the token lifecycle is tied to room membership. `join` creates an opaque token with at least 128 bits of cryptographically secure random entropy, stores it in server-private room state, and returns it in `JoinRoomResponse`. `leave` removes the member and invalidates the token.

Tokens are not profile data. They must never be added to `UserProfile`, `RoomSnapshot`, signalling `room-snapshot` messages, presence events, logs, public discovery responses, or error messages. The only response that returns a token is the successful `join` response for the joining user.

`lyre-web` extracts bearer tokens from HTTP requests and validates them before protected handlers mutate state. WebSocket signalling accepts `access_token` as a query parameter because browser WebSocket constructors cannot set arbitrary `Authorization` headers. The WebSocket route must be excluded from default full-URI request logging or wrapped so logs record only a redacted route path, never the raw `access_token` query.

The frontend stores the returned token in `sessionStorage` next to the current anonymous user and room id, then includes it in protected REST calls and the room WebSocket URL. The token is not placed in the share URL.

## Public Endpoints

These remain public:

- `GET /health`
- `GET /api/noise/providers`
- `GET /api/webrtc/ice-servers`
- `GET /api/webrtc/topology`
- `GET /api/rooms/{room_id}`
- `GET /api/rooms/{room_id}/media-relay`
- `POST /api/rooms/{room_id}/join`

## Protected Endpoints

These require a token bound to the same room and user:

- `POST /api/rooms/{room_id}/leave`
- `GET /api/rooms/{room_id}/ws?user_id=...&access_token=...`
- `POST /api/rooms/{room_id}/media-relay/start`
- `POST /api/rooms/{room_id}/media-relay/stop`
- `POST /api/rooms/{room_id}/media-relay/tracks`
- `POST /api/rooms/{room_id}/server-media/offer`
- `POST /api/rooms/{room_id}/server-media/candidates`
- `GET /api/rooms/{room_id}/server-media/candidates?user_id=...`
- `POST /api/rooms/{room_id}/server-media/close`

REST clients send the token as `Authorization: Bearer <access_token>`. Missing, malformed, unknown, room-mismatched, or user-mismatched tokens return `401 Unauthorized` with a JSON error body.

`start_media_relay` has no user id in its existing request body. It is protected by token possession for any current room member; the token's user id does not need to appear in the body.

## Protocol Changes

`JoinRoomResponse` gains:

- Rust/REST JSON: `access_token: string`
- WebRPC RIDL: `accessToken: string`
- Frontend adapted type: `access_token: string`

No protected endpoint bodies gain token fields. Token transport stays in HTTP headers or WebSocket query parameters.

## Frontend Behavior

The room client stores:

- `lyre.roomSession`, containing `{ roomId, user, accessToken }`

If the session value is missing, malformed, or its `roomId` does not match the current `/room/[roomId]` route, the room client joins again and replaces the stored session. When the stored `roomId`, `user`, and `accessToken` all exist and match the current route, it reuses them for the current tab session.

Protected frontend API helper signatures accept `accessToken` explicitly. This keeps token flow visible at call sites and avoids hidden global state.

## TURN and Noise Cancellation Boundary

`turn-rs` can be evaluated later as a Rust TURN server for NAT traversal and packaged as a deployment component if needed. TURN relays encrypted WebRTC packets and does not decode, denoise, or re-encode audio. Therefore TURN cannot provide server-side RNNoise or DeepFilterNet processing.

Server-side noise cancellation continues to require the existing server-media path where the backend terminates a WebRTC peer connection, decodes incoming audio to PCM, applies configured processing, and broadcasts processed audio to clients.

If Lyre later needs client-side noise cancellation, that work should use a Rust WASM package for browser audio processing, with the browser explicitly selecting client-side processing. It must not replace the server-side processing path required for broadcast-after-denoise.

## Testing

Core tests:

- `join` returns a non-empty token.
- tokens are distinct across separate joins.
- token validation accepts the joined room/user/token tuple.
- token validation rejects wrong room, wrong user, and unknown token.
- `leave` invalidates the departed user's token.
- public room snapshots do not serialize `access_token`.

Backend API tests:

- public media relay status remains available without `Authorization`.
- protected REST endpoint without `Authorization` returns `401`.
- protected REST endpoint with a token for a different user returns `401`.
- protected REST endpoint with the joined user's bearer token succeeds.
- WebSocket upgrade without `access_token` is rejected before upgrade.
- WebSocket room snapshot messages do not include `access_token`.
- route logging does not emit `access_token` for WebSocket requests.

Frontend tests:

- `JoinRoomResponse` type includes `access_token`.
- protected fetch helpers send `Authorization: Bearer <token>`.
- `roomSocketUrl` includes encoded `access_token`.
- room client stores and reuses `lyre.roomSession` only when its `roomId` matches the active route.
- room client passes the token to server relay, server media cleanup, leave, and WebSocket helpers.

## Documentation

Update `MEMORY.md` with the access-token design decision and the TURN/noise-cancellation boundary. Update `docs/roadmap.md` to move authentication and room access control into completed work and keep Rust WASM client-side noise cancellation as a future item.
