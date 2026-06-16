# Auto Audio Reconnect Mute Design

## Scope

Update the frontend room experience so server-relay audio starts automatically after a user joins a room, recovers from ICE interruption, and exposes a local microphone mute toggle instead of a manual audio connection button.

## Current Behavior

- `frontend/src/app/room/[roomId]/room-client.tsx` joins the room, opens the room WebSocket, then waits for the user to click `Connect audio`.
- `ServerMediaAudioSession` owns one WebRTC peer connection, local microphone stream, hidden playback element, local ICE candidate signalling, and server candidate polling.
- Room settings save closes and recreates the active audio session to apply updated microphone/browser DSP and server noise settings.

## Desired Behavior

- After room join and room WebSocket connection, the frontend automatically starts server relay audio once.
- The room toolbar no longer shows `Connect audio`.
- The former audio button becomes a `Mute` / `Unmute` button.
- Muting only affects the local microphone track by toggling local audio track `enabled`; it must not close the WebRTC session, stop tracks, alter server relay state, or mute remote playback.
- The default microphone state after joining is unmuted.
- If the browser ICE connection for the server-media peer reaches `disconnected` or `failed`, the frontend closes only the local `ServerMediaAudioSession` and starts a replacement session using the already-started server relay and already-registered media track.
- ICE reconnect must not call `startMediaRelay` or `registerMediaTrack` again for the same room session.
- Manual settings save while audio is active keeps its current behavior: update server relay settings, close the local audio session, and create a fresh local media session.

## API And Component Design

- Extend `ServerMediaAudioSession` with:
  - an optional `onConnectionInterrupted` callback fired when `iceConnectionState` becomes `disconnected` or `failed`;
  - `setMuted(muted: boolean)` to set every local audio track `enabled` to `!muted`.
- `RoomClient` remains the owner of relay lifecycle:
  - first automatic startup uses `updateRelay: false` and performs `startMediaRelay` plus `registerMediaTrack`;
  - reconnect/settings refresh use `updateRelay: true` and only recreate local browser media plus negotiation;
  - leave/startup-failure cleanup behavior stays unchanged.
- `RoomClient` tracks a local `muted` state and applies it to each newly created `ServerMediaAudioSession`.
- The reconnect path is serialized so repeated ICE state changes do not create overlapping sessions.

## Testing

Update frontend unit tests to cover:

- joining a room automatically starts server relay audio without clicking a connect button;
- `Connect audio` is absent and `Mute` / `Unmute` toggles the active local audio track enabled state;
- ICE `disconnected` or `failed` closes the old local session and creates a replacement session without repeating server relay start or track registration;
- startup failure cleanup and missing-signalling errors still surface and clean server media only when the initial relay startup had begun;
- settings save still updates relay settings and recreates local media.

## Documentation

- Update `docs/getting-started.md` so the frontend workflow says microphone/server-relay audio starts automatically after joining and the room toolbar exposes `Mute` / `Unmute`.
- Update `README.md` quick start text to remove the stale `Connect audio` instruction.
- Update `docs/roadmap.md` after implementation to record automatic frontend server-relay audio startup, ICE reconnect, and local microphone mute/unmute.

## Out Of Scope

- No peer mesh audio mode or peer-to-peer fallback.
- No persisted mute preference.
- No backend API changes.
- No new dependencies.
