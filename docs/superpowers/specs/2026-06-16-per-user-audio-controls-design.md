# Per-User Audio Controls Design

## Scope

Add room user controls so a listener can mute one other user and set that user's playback volume from 0% to 150%. The controls affect only the current listener. Muting also updates a server-side per-listener subscription so the server stops sending that source user's relay audio to this listener until unmuted. Volume remains browser-local gain and does not change server relay state, room membership, or other listeners.

## Current Context

Lyre room audio is server relay only. The frontend starts one `ServerMediaAudioSession`, receives relayed server media tracks, and currently attaches all remote tracks to one hidden `<audio>` element. The room user list renders nicknames only. Frontend settings and persisted browser state belong in `frontend/src/lib/settings-store.ts` through Zustand persistence.

The server WebRTC egress currently exposes one outbound audio track to each browser session. That is not enough for per-user controls because multiple source users can be written into the same receiver track. This feature changes server relay output to one outbound audio track per source user and adds a per-listener subscription API. The frontend can then decide which source users should be received and can apply local gain to each received source track.

## Requirements

- Each non-current user row in the room screen shows:
  - a mute/unmute button for that user's playback,
  - a volume range input constrained to 0 through 150,
  - a visible percentage label.
- The current user's own row does not show per-user playback controls.
- Muting a user sets that user's local playback to silence, updates the server subscription to exclude that source user for this listener, and does not change the local microphone mute button.
- Unmuting restores that user's prior configured volume.
- Unmuting updates the server subscription to include that source user again.
- Setting volume to 0 is allowed and is distinct from the mute flag; raising volume from 0 does not automatically clear mute.
- User playback preferences persist through the existing Zustand `lyre.settings` storage key under user ID keys.
- Existing persisted settings that do not contain playback preferences hydrate with defaults.
- Defaults are `muted: false` and `volumePercent: 100`.
- Volume values are clamped to the allowed range before storage and before applying to an audio element.
- Existing automatic server relay connect/reconnect behavior remains unchanged.
- Subscription updates do not require adding peer mesh signalling.
- If subscription changes require browser media renegotiation, the frontend closes and recreates its existing server-media session through the current server-media offer/answer flow rather than adding dynamic peer mesh negotiation.
- When a remote user joins, the default subscription includes that user unless the listener already has that user muted in persisted browser settings.
- When a remote user leaves, the frontend removes that user's playback audio element and controls with the existing presence update.
- Reconnecting server-media playback reapplies the current subscription and per-user volume settings before playback.
- Persisted muted users are excluded before the first server-media session is negotiated.
- If a subscription update fails, the frontend reports the error through the existing room status text and does not claim the subscription changed.
- Incoming remote tracks with missing or invalid Lyre source-user track IDs are ignored and reported through the existing audio error/status path; they are not played through shared fallback audio.
- Server relay remains the only audio topology; no peer mesh fallback is added.

## Architecture

`settings-store.ts` owns a `userAudio` map keyed by user ID. Each entry stores `muted` and `volumePercent`, with store actions for replacing one user's settings and clearing one user's settings. The room UI reads that map, renders per-user controls for remote users, pushes the full map to the active `ServerMediaAudioSession` when preferences or the session changes, and sends the current unmuted remote user IDs to the server subscription API.

`ServerMediaAudioSession` changes from one shared hidden audio element to per-source playback paths. On `RTCPeerConnection.ontrack`, it parses the source user ID from the remote track ID. It creates or reuses one `MediaStream` plus one browser-local gain path per source user, applies the current source user's settings, and starts playback. The gain path uses Web Audio `MediaStreamAudioSourceNode -> GainNode -> AudioContext.destination` so 150% gain is representable; the hidden `<audio>` element is only used if the browser test/runtime path requires media element playback priming, not for gain above 100%.

The WebRTC server egress track ID becomes `lyre-user:<encoded-user-id>:audio`, where `<encoded-user-id>` uses percent encoding for any byte outside an unreserved ASCII set. The browser parser accepts only that format and decodes the user ID. Tracks without a parseable source user are ignored and reported through the existing status path rather than played through a shared fallback.

On the Rust side, `MediaRelayRegistry` or a focused adjacent relay-state module owns per-room, per-recipient subscriptions as a set of source user IDs. The default is subscribed to every source, so existing behavior is preserved for clients that never call the subscription API. A new authenticated REST/WebRPC-compatible route updates one recipient's subscribed source user IDs after validating the recipient and source users are active room participants.

`ProcessedAudioEgressFanout` and raw Opus forwarding consult the subscription state before sending audio from a source user to a recipient. `ServerMediaEgress` owns a map from source user ID to `{TrackLocalStaticRTP, encoder}` and emits packets for different speakers on different outbound tracks using `lyre-user:<encoded-user-id>:audio` track IDs.

Outbound tracks are created before the server answers the browser's offer. When `POST /api/rooms/{room_id}/server-media/offer` handles a recipient, it reads that recipient's current subscribed source user IDs from the media relay state, excluding the recipient's own user ID, and creates one outbound audio track per source before generating the answer. This avoids adding tracks after negotiation and avoids dynamic WebRTC renegotiation. If a listener changes the subscribed source set while audio is connected, the frontend closes the current server-media session and recreates it through the existing offer/answer path after the subscription update succeeds.

## Subscription API Contract

REST endpoint:

```text
POST /api/rooms/{room_id}/media-relay/subscriptions
Authorization: Bearer <room access token>
Content-Type: application/json
```

Request body:

```json
{
  "user_id": "listener_user_id",
  "source_user_ids": ["source_user_a", "source_user_b"]
}
```

Response body:

```json
{
  "room_id": "DEFAULT",
  "user_id": "listener_user_id",
  "source_user_ids": ["source_user_a", "source_user_b"]
}
```

Behavior:

- `user_id` is the listener/recipient whose subscription is being changed.
- `source_user_ids` is the complete desired source-user allow-list for that listener.
- The route is idempotent: sending the same list again returns the same subscription.
- The server sorts and deduplicates `source_user_ids` in storage and response.
- The route requires the bearer token for `user_id`; another room member's token is not enough.
- The route fails with the existing API error shape/status if the media relay is inactive, if `user_id` is not an active relay participant, or if any listed source user is not an active relay participant.
- An empty `source_user_ids` list is valid and means the listener receives no remote user audio tracks.
- WebRPC/RIDL gets matching DTOs and an `UpdateMediaRelaySubscriptions` method documenting the REST-compatible shape. The frontend runtime may continue using the REST wrapper, matching current server-media REST behavior.

Server-media offer behavior:

- Before the frontend starts or recreates `ServerMediaAudioSession`, it sends the current subscription list.
- `answerServerMediaOffer` uses that stored list to create outbound source tracks before answer generation.
- If the stored list changes later, no in-place renegotiation is attempted; the frontend closes and recreates the server-media session.
- If a new remote user joins while audio is connected and the listener is not muted for that user, the frontend updates the subscription list and recreates the server-media session.
- If a remote user leaves while audio is connected, the frontend updates local controls, sends a subscription list without that user, and recreates connected server-media playback only when the effective subscription list changes; no playback fallback is created for stale tracks.

## UI Design

The room user list remains a compact operational panel. Each remote row uses a simple two-column responsive layout: nickname on the left, controls on the right. The mute button uses the existing button primitive. The volume input is a native range slider with fixed bounds and an adjacent percentage label to avoid adding new UI dependencies.

## Tests

- `settings-store.test.ts` covers default `userAudio`, persistence, hydration of older settings, volume clamping, and per-user updates.
- `server-media-audio.test.ts` covers source user parsing, per-user gain path creation, muted/volume gain application up to 1.5, ignoring invalid source-user track IDs, and playback path cleanup.
- `room-client.test.tsx` covers rendering controls only for remote users, muting one remote user, changing one user's volume, persisted muted users before first connect, subscription update failure reporting, remote join/leave subscription behavior, reconnect applying current preferences, updating subscriptions, and recreating server-media playback after subscription changes when audio is connected.
- Frontend API tests cover the subscription route request shape and auth token handling.
- Rust API/media tests cover subscription validation, default subscribe-all behavior, persisted fanout filtering, and raw Opus filtering.
- Rust tests in `crates/lyre-webrtc` cover the advertised egress track ID format, source user encoding, pre-answer per-source track creation, and routing processed/raw egress packets for different source users to different outbound tracks.

## Documentation Impact

Update `docs/roadmap.md` after implementation to list per-user frontend playback mute/volume controls as completed. Update `docs/api-contracts.md` with the subscription endpoint, request body, response body, and auth behavior. Update `proto/lyre.ridl` and generated TypeScript; if the generator cannot run in the local environment, manually keep the generated TypeScript contract aligned and report the generator failure in final verification.

## Out Of Scope

- Server-side per-listener gain.
- Persisting subscriptions across sessions beyond the existing in-memory/room state lifetime.
- Global output device selection.
- Keyboard shortcuts or advanced audio metering.
- Peer mesh audio compatibility.
