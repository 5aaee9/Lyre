# Listen-Only No-Microphone Design

## Scope

When a browser has no usable microphone, Lyre should still let the client enter the room audio path in listen-only mode. The client must subscribe to server-relayed remote audio, avoid registering a fake local audio track, and make the UI clear that the local microphone is unavailable.

This builds on the stale saved-device fallback in `frontend/src/lib/webrtc.ts`: a missing saved microphone still retries the browser default first. Listen-only starts only when microphone capture still fails with a missing-input error after that retry.

## Current Behavior

- `RoomClient` automatically starts server relay audio after the room WebSocket opens.
- Audio startup calls `openLocalAudioStream()`, then starts the media relay, registers `audio-main`, updates subscriptions, and negotiates `ServerMediaAudioSession`.
- If `getUserMedia({ audio: ... })` fails because the browser has no microphone, the catch path resets `audioStarted` and displays the browser error such as `Requested device not found`.
- `ServerMediaAudioSession` creates a peer connection through `createPeerConnection()`, which only adds local audio tracks. With an empty stream, a real browser offer may contain no audio m-line, so the server cannot answer with remote audio.
- `MediaRelayRegistry` currently treats a participant as registered only after `register_track()`. A listen-only user that skips local track registration would fail subscription and server-media offer checks with `ParticipantNotFound`.

## Design

### Server Relay State

Add an explicit media relay participant registration path that creates a participant with zero tracks:

- Core method: `MediaRelayRegistry::register_participant(room_id, user_id)`.
- REST route: `POST /api/rooms/:room_id/media-relay/participants` with `{ "user_id": "..." }`.
- WebRPC contract method: `RegisterMediaParticipant(roomID: string, userID: string) => (mediaRelay: MediaRelayRoomStatus)`.
- Frontend wrapper: `registerMediaParticipant(roomId, userId, accessToken)`.

This endpoint is for clients that need relay subscription/server-media state without publishing a local source. It must require an active relay and the same room-user authorization as track registration. Calling `registerMediaTrack()` remains the publishing path and still creates or updates the participant with the supplied track.

Do not change the server relay topology and do not reintroduce peer mesh audio.

### Subscribable Sources

Listen-only participants must not be treated as remote audio sources. The server should define subscribable audio sources as participants with at least one registered `MediaTrackKind::Audio` track.

Server subscription behavior should use that audio-source definition:

- `MediaRelayRegistry::subscriptions()` defaults to all remote participants with at least one audio track.
- `MediaRelayRegistry::update_subscriptions()` rejects explicit source IDs for participants that have no audio track with the existing `ParticipantNotFound` error.
- `MediaRelayRegistry::is_source_subscribed()` returns `false` for non-audio-source participants instead of treating listen-only users as implicitly subscribed sources.

Frontend relay source discovery should also include only participants with at least one `audio` track and should still exclude the current user. This keeps the UI and request payloads aligned with the server contract, while the server remains the source of truth.

The server can still represent listen-only users as relay participants with an empty `tracks` array for diagnostics and cleanup.

### Frontend Audio Startup

`RoomClient` should classify missing microphone errors narrowly:

- `NotFoundError`
- `OverconstrainedError` only when `constraint === "deviceId"`

On those errors from `openLocalAudioStream()`:

1. Create an empty `MediaStream`.
2. Mark the connection as listen-only for this audio session.
3. Start the media relay if needed.
4. Register the current user as a media relay participant with no track.
5. Skip `registerMediaTrack("audio-main", "audio")`.
6. Update subscriptions from active remote audio sources.
7. Negotiate `ServerMediaAudioSession` in receive-only mode.
8. Skip local `VoiceActivityDetector`.
9. Show a translated `Listening without microphone` status and render the current user row as listen-only.
10. Disable the local mute button while listen-only, because there is no local microphone track to mute.

On a normal microphone stream, existing behavior remains: register `audio-main`, start local voice activity detection, and allow local mute/unmute.

Permission denial, generic media errors, ICE failures, signalling failures, server errors, and cleanup behavior should stay on the existing failure paths. Listen-only must not swallow those errors.

### WebRTC Offer Shape

Add an option to the client WebRTC peer creation path so server-media sessions can request a receive-only audio transceiver when there are no local audio tracks:

- `createPeerConnection(iceServers, stream, { receiveOnlyAudio: true })`
- If the stream has local audio tracks, add tracks as today and do not add a receive-only transceiver.
- If the stream has no local audio tracks and `receiveOnlyAudio` is true, call `RTCPeerConnection.addTransceiver("audio", { direction: "recvonly" })`.

`ServerMediaAudioSession` should accept a `listenOnly` boolean and pass this option when constructing the peer connection.

### UI and Messages

Add Room messages for:

- `listeningWithoutMicrophone`
- `listenOnly`

`RoomStatusBadge.translateStatus()` should translate `Listening without microphone`. The current user's participant subtitle should use `listenOnly` when listen-only is active; otherwise it should remain `localMicrophone`.

## Acceptance Criteria

- A no-microphone browser enters listen-only after `openLocalAudioStream()` rejects with `NotFoundError` or `OverconstrainedError` on `deviceId`.
- Listen-only startup calls `startMediaRelay()`, `registerMediaParticipant()`, `updateMediaRelaySubscriptions()`, and `answerServerMediaOffer()`.
- Listen-only startup does not call `registerMediaTrack()` and does not start local voice activity detection.
- Listen-only server-media negotiation adds a receive-only audio transceiver when the local stream has no audio tracks.
- The listen-only UI shows the translated `Listening without microphone` status, marks the current user as listen-only, and disables the local mute button.
- Permission-denied microphone failures still fail audio startup, do not register a media relay participant, and keep the error visible.
- Server default subscriptions, explicit subscription updates, `is_source_subscribed()`, and frontend relay source discovery ignore participants with no `audio` tracks as audio sources.
- Existing microphone-equipped startup behavior continues to register `audio-main`, subscribe to remote audio sources, start local voice activity detection, and allow mute/unmute.
- Leaving the room or closing server-media state still removes listen-only participants through the existing participant cleanup path.
- REST, WebRPC, frontend API wrapper, and generated frontend WebRPC types remain consistent: `proto/lyre.ridl` includes `RegisterMediaParticipant`, Rust WebRPC DTOs/handlers/routes wire `/rpc/Lyre/RegisterMediaParticipant`, `frontend/src/lib/lyre.gen.ts` is regenerated with `npm run generate:webrpc`, and the generated diff is committed.
- `docs/api-contracts.md`, `docs/media-architecture.md`, and `docs/roadmap.md` record the new participant registration endpoint and listen-only no-microphone support after implementation review approval.

## Tests

- `crates/lyre-core/src/media_tests.rs`: prove `register_participant()` requires an active relay, creates an empty-track participant, allows that participant to update subscriptions as a listener, excludes empty-track participants from default audio sources, rejects explicit empty-track source IDs with `ParticipantNotFound`, returns false for `is_source_subscribed()` on empty-track sources, and is cleaned up by existing removal.
- `crates/lyre-web/src/api_media_tests.rs`: prove the REST participant registration route uses auth and returns an empty-track participant.
- `crates/lyre-web/src/webrpc_tests/media_relay.rs`: prove the WebRPC participant registration method returns the wrapper shape.
- `frontend/src/lib/api.test.ts`: prove `registerMediaParticipant()` serializes the expected REST request.
- `frontend/src/lib/webrtc.test.ts`: prove receive-only peer creation adds `addTransceiver("audio", { direction: "recvonly" })` only when requested and no local audio tracks exist.
- `frontend/src/app/room/[roomId]/room-client.test.tsx`: prove the listen-only path, permission-denied path, and source filtering behavior.
- `cd frontend && npm run generate:webrpc && git diff --exit-code -- src/lib/lyre.gen.ts` is run after RIDL changes, or the generated file is intentionally updated and included in the final diff.
- Existing focused and full verification commands continue to pass.

## Rejected Alternatives

- Register a fake `audio-main` track for no-microphone clients. This would make listeners look like audio publishers and could mislead subscriptions, diagnostics, and egress logic.
- Generate a silent local audio track in the browser. This adds unnecessary audio graph complexity and still publishes a meaningless source.
- Treat all `getUserMedia` failures as listen-only. Permission denial and non-missing media failures should remain visible because they indicate different user or browser states.
- Skip server relay participant registration entirely. Current server-media subscriptions require participant state for the recipient user, so this would fail before negotiation.
