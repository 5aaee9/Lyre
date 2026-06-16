# WebSocket Disconnect Room Leave Design

## Problem

When a user closes a browser tab, navigates away, or otherwise loses the room page without pressing Leave, the browser unmount cleanup closes the WebSocket but does not call the authenticated leave REST endpoint. The server currently removes the closed socket from `PeerHub`, but the user's `RoomRegistry` entry remains in the room snapshot. Other users still see the disconnected user in the room user list until an explicit leave happens.

## Goal

Make the server WebSocket lifecycle authoritative for unexpected room departures: when an authenticated room WebSocket ends, the server removes that user from the room registry, persists the room-state change when persistence is configured, records leave metrics only when a user was actually removed, and broadcasts the existing `user-left` signal to remaining peers. The frontend must also stop reusing a room session after its WebSocket has closed, because the server-side disconnect invalidates that session's room membership and access token.

## Non-Goals

- Do not add browser `beforeunload`, `pagehide`, `sendBeacon`, or extra client-side leave fallbacks for correctness.
- Do not change the room access token model.
- Do not change the user-facing room UI.
- Do not remove explicit Leave button behavior; it remains the intentional path that also closes per-user server media before leaving the room.
- Do not add peer mesh audio behavior or compatibility fallbacks.

## Current Behavior

- `RoomClient` joins through `POST /api/rooms/{room_id}/join`, stores the returned user and room access token in `sessionStorage`, and opens `/api/rooms/{room_id}/ws`.
- Component unmount cleanup closes local audio and closes the WebSocket, but intentionally does not call `leaveRoom` or per-user server-media cleanup.
- `RoomClient` currently keeps `lyre.roomSession` in `sessionStorage` on unmount, so a same-tab revisit can try to reuse a user/token pair that the server has removed after WebSocket disconnect cleanup.
- `handle_socket` in `crates/lyre-web/src/api.rs` registers the socket in `PeerHub`, sends a room snapshot, forwards messages while the socket is alive, and calls `state.peers.disconnect(&room_id, &user_id)` after the receive/send loop exits.
- `PeerHub::disconnect` removes the socket sender and broadcasts `user-left`, but it does not mutate `RoomRegistry`.
- `AppState::leave_room_persisted` already centralizes registry removal, optional JSON persistence, leave metrics, and rollback on persistence failure.

## Proposed Design

Add an `AppState::disconnect_room_socket(&self, room_id: &RoomId, user_id: &UserId)` helper for WebSocket teardown. The helper should:

1. Remove the user from `RoomRegistry` through the same persistence and metrics path used by REST Leave.
2. Preserve the `RoomRegistry::leave` removal status in the returned/internal result.
3. Remove the socket sender from `PeerHub` in every case where teardown runs.
4. Broadcast `user-left` only when the persisted registry leave succeeded and `removed` is true.

Use this exact contract:

```rust
pub async fn leave_room_persisted(
    &self,
    room_id: &RoomId,
    user_id: &lyre_core::UserId,
) -> Result<lyre_core::LeaveRoomResponse, ApiError>

pub async fn disconnect_room_socket(&self, room_id: &RoomId, user_id: &lyre_core::UserId)
```

`leave_room_persisted` should return the full `LeaveRoomResponse` so REST, WebRPC, and WebSocket cleanup can all see whether membership was actually removed. REST and WebRPC leave handlers should convert `response.room` into their existing response bodies and broadcast `user-left` only when `response.removed` is true. Metrics should continue to increment only when `LeaveRoomResponse.removed` is true.

`disconnect_room_socket` should absorb persistence errors because there is no open WebSocket response path after teardown. It should log the full error chain with `{error:#}`, remove the peer socket with `PeerHub::remove_peer`, and skip `user-left` broadcast when `leave_room_persisted` returns an error or returns `removed: false`.

The WebSocket disconnect path should preserve lower-level error context in logs if persisted leave fails. Because the socket is already gone, the server cannot report the failure to that client. In that failure case, it should still remove the peer socket from `PeerHub` so the closed connection cannot receive future signals, but it should not broadcast `user-left` unless the registry leave succeeded. This avoids telling peers the room state changed when persistence rollback kept the user in the room.

Split `PeerHub` cleanup into two explicit operations:

- `remove_peer(room_id, user_id)` removes only the socket sender and does not broadcast.
- `disconnect(room_id, user_id)` keeps the current behavior by calling `remove_peer` and then broadcasting `user-left`.

`disconnect_room_socket` should call `remove_peer` first or otherwise guarantee the socket is gone, then call `user_left` only when the persisted leave actually removed membership.

Explicit REST Leave must avoid the WebSocket/REST authorization race. `RoomClient.leave` should close local audio and per-user server media first, then call `leaveRoom` while the access token is still valid, then remove `lyre.roomSession`, close the WebSocket, and navigate away. The eventual WebSocket teardown will see the registry membership is already gone and should only remove the socket without incrementing metrics or broadcasting a duplicate `user-left`.

`RoomClient` should remove `lyre.roomSession` from `sessionStorage` when the room WebSocket closes and during component unmount cleanup. It should not call REST Leave from browser unload/unmount paths. This keeps revisits simple: if the user returns to the room in the same tab after an accidental departure, `readRoomSession` finds no stored session and the existing join flow creates a fresh user, access token, and WebSocket.

## Acceptance Criteria

- Closing or losing a WebSocket for a joined user removes that user from `RoomRegistry`.
- Remaining connected peers receive a `user-left` signal after the disconnected user is removed from the registry.
- A subsequent room snapshot after the WebSocket disconnect no longer includes the disconnected user.
- JSON room-state persistence is updated on WebSocket disconnect when persistence is configured.
- If JSON room-state persistence fails during WebSocket disconnect, the registry rollback keeps the user membership and token, leave metrics do not increment, the closed peer socket is removed, and no false `user-left` signal is broadcast.
- Leave metrics increment once when the WebSocket disconnect removes a user, and do not increment for duplicate disconnect/leave cleanup after the user is already gone.
- REST Leave and WebRPC Leave preserve their existing response shapes while using the split leave helper and broadcasting `user-left` only when `removed` is true.
- Explicit Leave calls the authenticated REST leave endpoint before closing the room WebSocket, so WebSocket cleanup cannot invalidate the token before REST authorization.
- WebSocket cleanup after an explicit REST leave removes only the socket sender and does not double-count metrics or broadcast a duplicate `user-left`.
- The WebSocket disconnect path logs persisted leave failures with the full error chain and still removes the closed socket from `PeerHub`.
- Explicit Leave button behavior continues to close per-user server media before calling the REST leave endpoint.
- Existing client unmount cleanup remains local-only and does not add unreliable browser unload leave requests.
- After WebSocket close or room component unmount, the frontend removes the stored `lyre.roomSession` entry so a same-tab revisit joins fresh instead of reusing an invalid token.

## Testing

- Add Rust unit coverage around `PeerHub` for a disconnect variant that removes only the socket without broadcasting, so failed persisted leave can still clean up hub state without emitting a false `user-left`.
- Add Rust async coverage for WebSocket disconnect cleanup using `AppState` directly: join two users, connect both to `PeerHub`, call the new disconnect cleanup for one user, assert the registry snapshot has only the remaining user, assert the remaining peer receives `user-left`, and assert the leave metric increments once.
- Add Rust async coverage for persistence-configured WebSocket disconnect success: after disconnect cleanup, assert the JSON state file no longer contains the disconnected user's ID or access token.
- Add Rust async coverage for persistence-save failure during WebSocket disconnect: configure an always-failing persistence writer, assert the registry still validates the user's token after rollback, assert the remaining peer receives no `user-left`, assert the closed peer is removed from `PeerHub`, and assert leave metrics do not increment while persistence failure metrics do.
- Add Rust async coverage for the duplicate cleanup case: remove the user first through `leave_room_persisted`, then run WebSocket disconnect cleanup and assert metrics do not increment again.
- Add or update WebRPC room tests proving WebRPC Leave still returns its existing shape after `leave_room_persisted` starts returning `LeaveRoomResponse`.
- Add frontend RoomClient coverage proving explicit Leave calls `leaveRoom` before closing the WebSocket.
- Update frontend RoomClient coverage proving unmount does not call `leaveRoom` or server-media cleanup and does clear the stored room session.
- Add frontend RoomClient coverage proving WebSocket close clears the stored room session.

## Documentation Impact

Update `docs/roadmap.md` Completed with a concise entry that room membership is now cleaned up by authenticated WebSocket disconnects.
