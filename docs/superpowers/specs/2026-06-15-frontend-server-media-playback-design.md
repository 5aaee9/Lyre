# Frontend Server Media Playback Design

## Scope

Build the first browser-facing server-media audio path for the existing `/room/[roomId]` page.

This increment only wires the frontend to already-existing backend REST/WebRTC boundaries:

- `POST /api/rooms/{room_id}/media-relay/start`
- `POST /api/rooms/{room_id}/media-relay/tracks`
- `POST /api/rooms/{room_id}/server-media/offer`
- `POST /api/rooms/{room_id}/server-media/candidates`
- `GET /api/rooms/{room_id}/server-media/candidates?user_id={user_id}`

It does not add DeepFilterNet, jitter buffering, packet loss concealment, authentication, persistence, new backend debug endpoints, or Rust WASM client-side noise cancellation.

## User Experience

- The room route remains `/room/[roomId]`.
- The room page keeps the existing presence WebSocket and user list behavior.
- The page exposes an audio mode control with `Server relay` as the default and `Peer mesh` as an explicit compatibility option.
- The audio mode control is disabled while audio is connected. Users leave and re-enter the room, or reconnect after cleanup, to change modes.
- Clicking `Connect audio` in server relay mode requests the microphone, negotiates one browser-to-server WebRTC peer connection, starts the room media relay using the user's stored noise config, registers the local audio track as `audio-main`, and plays the processed server audio returned by the backend peer connection.
- Clicking `Connect audio` in peer mesh mode preserves the existing mesh behavior.
- Leaving the room closes the active audio session, stops local tracks, closes the WebSocket, and calls the existing room leave endpoint.
- Unmounting closes local browser audio resources and the WebSocket. It does not call `leaveRoom` or `stopMediaRelay` because route unmount can happen during browser navigation where fire-and-forget mutation reliability is poor; the explicit Leave button owns server-side teardown in this increment.
- Errors from ICE loading, microphone capture, offer negotiation, candidate exchange, relay start, track registration, and browser playback setup remain visible in the room status text.

## Frontend Architecture

Add `frontend/src/lib/server-media-audio.ts` with a small `ServerMediaAudioSession` class. It owns one `RTCPeerConnection`, the local microphone stream, a hidden `HTMLAudioElement`, and a polling timer for server ICE candidates.

Responsibilities:

- Create an `RTCPeerConnection` using the existing ICE server mapping.
- Add local audio tracks to the connection.
- Attach remote tracks to an internal `MediaStream` and call `audio.play()` when server audio arrives.
- Send browser ICE candidates through `addServerMediaIceCandidate`.
- Poll `getServerMediaIceCandidates` on a short interval and add unseen server candidates to the peer connection.
- Create an offer, send it through `answerServerMediaOffer`, set the returned answer as the remote description, and fetch candidates once immediately after negotiation.
- Close the peer connection, stop local tracks, stop candidate polling, clear the audio element source, and remove the audio element from the DOM.

The existing `MeshAudioSession` remains unchanged except for any shared helper extraction that is strictly needed.

## Room Page Integration

`frontend/src/app/room/[roomId]/room-client.tsx` will manage two mutually exclusive audio session refs:

- `MeshAudioSession` for peer mesh mode.
- `ServerMediaAudioSession` for server relay mode.

Server relay startup order:

1. Load ICE servers.
2. Request local microphone audio.
3. Start the media relay with `readNoiseConfig()`.
4. Register `audio-main` for the current user.
5. Construct `ServerMediaAudioSession`.
6. Negotiate the server-media offer/answer.
7. Set status to `Server relay audio connected`.

If any step fails, close any partially created server-media session, stop the microphone tracks, reset the started flag, and show the underlying error message.

Presence signalling remains active for room membership. WebRTC offer/answer/ICE messages from the mesh signalling socket are ignored unless peer mesh mode is active.

## Cleanup Semantics

- `stopMediaRelay(roomId, userId)` is a room-level backend stop today: it stops the room relay, clears processed media, and closes all server-media sessions for that room. The frontend must not call it automatically for one user's failure, Leave click, or unmount in this increment because that would interrupt other participants.
- Server relay startup failure after `startMediaRelay` performs local cleanup only and leaves room-level relay cleanup to a future backend per-user server-media cleanup endpoint.
- The explicit Leave button calls the existing `leaveRoom` endpoint only. It does not call `stopMediaRelay` in server relay mode until a per-user cleanup API exists.
- Component unmount only performs local cleanup: close the audio session, stop local tracks, stop ICE candidate polling, remove the playback element, and close the WebSocket.
- Audio mode switching is disabled after audio starts, so no live close-and-switch path is added in this increment.
- Playback is remote-participant audio only. The existing server egress fanout excludes the source user, so a single connected user should negotiate and attach the playback element but should not expect self-loopback audio.

## Testing

Add Vitest coverage for:

- `ServerMediaAudioSession` negotiates an offer, posts local candidates, polls server candidates, attaches remote audio, and closes cleanly.
- `ServerMediaAudioSession` deduplicates repeated server candidates during polling.
- The room page defaults to server relay mode and, on `Connect audio`, calls ICE loading before microphone capture, starts the media relay with stored noise settings, registers `audio-main`, negotiates server media, and does not send mesh signalling offers.
- Selecting peer mesh mode keeps the existing mesh negotiation behavior.
- Server relay startup failures leave no active session and stop local tracks.
- The room page does not call `stopMediaRelay` on server relay startup failure, explicit Leave, unmount, or peer mesh mode because the current backend stop is room-level.
- The server relay playback test should assert remote-track attachment/playback setup, not self-loopback audio from the same user.

Existing API wrapper tests remain in place; no new backend endpoints are required.

## Documentation

- Update `MEMORY.md` with the server relay frontend design decision, including that unmount and Leave only perform local browser cleanup for server relay media because the existing stop endpoint is room-level.
- Update `docs/roadmap.md` by moving frontend server-media playback from Next to Completed and keeping the remaining backend media quality items in Next.
- Keep a roadmap item for per-user server-media cleanup so the frontend can later release server resources without stopping the whole room relay.

## Acceptance Criteria

- `/room/[roomId]` remains the shareable room URL.
- Server relay mode is the default room audio path.
- Peer mesh mode remains available and tested.
- Browser-to-server WebRTC negotiation uses the existing REST server-media APIs.
- Processed server audio is attached to an audio element for browser playback.
- Local microphone tracks and polling timers are cleaned up on failure, leave, and unmount.
- Explicit Leave does not call the room-level media relay stop endpoint; unmount does not perform server mutations.
- Server relay playback means remote participant audio only, with no self-loopback requirement.
- Audio mode cannot be changed while an audio session is active.
- `MEMORY.md` and `docs/roadmap.md` are updated after implementation review approval.
- Frontend `npm test -- --run`, `npm run typecheck`, `npm run lint`, and `npm run build` pass.
- Rust workspace verification still passes because no backend behavior changes are expected.
