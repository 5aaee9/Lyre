# Per-User Server Media Cleanup Design

## Scope

Add a per-user cleanup path for server-media clients so one browser can leave or fail server-media startup without stopping the entire room media relay.

This increment covers:

- Closing one `ServerMediaSessionKey { room_id, user_id }`.
- Stopping that user's server-media runtime pump.
- Removing that user's media relay participant tracks while keeping the room relay active and preserving room noise settings.
- Exposing the cleanup through a REST endpoint documented in the WebRPC RIDL contract.
- Calling the cleanup from the frontend server relay flow on explicit Leave and on server relay startup failures after relay/track registration.

This increment does not add authentication, persistence, room ownership, automatic cleanup on component unmount, browser `sendBeacon`, DeepFilterNet, jitter buffering, packet loss concealment, or TURN changes.

## Backend Behavior

Add a media relay registry operation that removes one participant from an active room:

- Input: `room_id`, `user_id`.
- If the room relay is active, remove the participant and all of their tracks.
- Keep the room relay status active.
- Keep room-level noise configuration unchanged.
- Return the normal `MediaRelayRoomStatus` snapshot.
- If the room does not exist or is inactive, return the existing inactive relay error shape rather than creating a room.

Add an AppState operation for per-user server-media cleanup:

- Stop the `ServerMediaRuntimePump` task for the exact room/user key.
- Close the exact server-media peer/session via `ServerMediaNegotiator::close(&key)`.
- Remove the media relay participant for that user.
- Do not stop `ProcessedAudioWebRtcEgressPump` for the room.
- Do not close other users' server-media sessions or peer handles.
- Return a response that includes the resulting `MediaRelayRoomStatus` and the closed `ServerMediaSessionStatus` when a session existed.

Add a REST endpoint:

```text
POST /api/rooms/{room_id}/server-media/close
body: { "user_id": "..." }
response: {
  "media_relay": MediaRelayRoomStatus,
  "session": ServerMediaSessionStatus | null
}
```

The endpoint is idempotent for missing server-media sessions: it still removes the relay participant when possible and returns `"session": null`.

## Frontend Behavior

Add an API wrapper:

```ts
closeServerMediaSession(roomId: string, userId: string): Promise<CloseServerMediaSessionResponse>
```

The wrapper uses the new REST endpoint and throws a visible error on non-2xx responses.

Update `ServerMediaAudioSession.close()` to remain local-only. Network cleanup is owned by the room client, not by the low-level session class.

Update the room page server relay flow:

- Track whether server relay startup reached the point where server cleanup is meaningful.
- On explicit Leave, close local media resources first, then call `closeServerMediaSession(roomId, currentUser.id)` when server relay mode started, then call `leaveRoom`.
- On server relay startup failure after `startMediaRelay` or `registerMediaTrack`, call `closeServerMediaSession(roomId, currentUser.id)` before returning control to the user.
- If cleanup itself fails during a startup failure, keep the original startup error visible in room status.
- Component unmount remains local-only and does not call server mutation endpoints.
- Peer mesh mode never calls the server-media cleanup endpoint.

## WebRPC Contract

Update `proto/lyre.ridl` and regenerate `frontend/src/lib/lyre.gen.ts` so the generated TypeScript contract documents:

- `CloseServerMediaSession(roomID: string, userID: string) => (closed: CloseServerMediaSessionResponse)`
- `CloseServerMediaSessionResponse` with `mediaRelay: MediaRelayRoomStatus` and optional `session: ServerMediaSessionStatus`.
- `ServerMediaSessionStatus` if it is not already present in the generated contract.

Runtime transport remains REST fetch wrappers in this increment.

## Testing

Add Rust tests for:

- `MediaRelayRegistry` removes one participant while keeping relay active, preserving noise, and keeping other participants.
- Removing a participant from an inactive or missing relay returns `MediaRelayError::Inactive` without creating the room.
- `ServerMediaNegotiator::close(&key)` already closes one peer; add coverage only if existing tests do not prove it preserves other users.
- `AppState` per-user cleanup stops only the matching runtime pump and server-media peer, removes only that relay participant, and leaves room egress pump active.
- REST route `/api/rooms/{room_id}/server-media/close` returns the expected media relay status and optional session.
- The route is idempotent for a missing server-media session when the relay participant exists.

Add frontend tests for:

- `closeServerMediaSession` serializes the close body and throws on non-2xx responses.
- Server relay explicit Leave calls `closeServerMediaSession` before `leaveRoom` and never calls room-level `stopMediaRelay`.
- Server relay startup failure after relay/track registration calls `closeServerMediaSession` and keeps the original startup error visible if cleanup succeeds or fails.
- Component unmount does not call `closeServerMediaSession`.
- Peer mesh mode does not call `closeServerMediaSession`.

## Documentation

After implementation review approval:

- Update `MEMORY.md` with the per-user cleanup decision and the fact that unmount remains local-only.
- Update `docs/roadmap.md` by moving per-user server-media cleanup to Completed.

## Acceptance Criteria

- One user's server-media cleanup does not stop the room media relay.
- One user's cleanup does not close other users' server-media sessions or peer handles.
- The user's media relay participant tracks are removed.
- Explicit Leave in server relay mode releases server resources before leaving room presence.
- Startup failures after server relay registration clean up server resources without hiding the original error.
- Unmount stays local-only.
- WebRPC RIDL and generated TypeScript contract include the cleanup response.
- Rust fmt, clippy, nextest pass.
- Frontend WebRPC generation, tests, typecheck, lint, and build pass.
