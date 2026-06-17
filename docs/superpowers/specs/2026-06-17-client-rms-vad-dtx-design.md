# Client RMS Voice Activity and Opus DTX Design

## Goal

Show per-user active speaker state in the room UI using only browser-side audio analysis, and enable Opus DTX when the browser permits it to reduce upstream bandwidth during silence.

## Scope

This increment is frontend-only except for documentation. It does not add a `user-speaking` WebSocket message, does not require the Rust server to inspect audio packets, and does not change media relay DTOs or WebRPC contracts.

## Current Context

Room audio is server-relay only. The frontend sends local microphone audio through `ServerMediaAudioSession`, receives one remote WebRTC audio track per source user, and identifies remote sources from track IDs shaped like `lyre-user:<encoded-user-id>:audio`. Existing per-user mute and volume controls are applied in the browser with `GainNode`s.

## Architecture

Add a small frontend voice activity detector module that attaches an `AnalyserNode` to a `MediaStream`, samples time-domain audio, computes RMS energy, and emits speaking state changes after debounce thresholds.

`RoomClient` owns the displayed speaking state as a `Set<string>` of user IDs. It starts one detector for the local microphone stream after audio starts. `ServerMediaAudioSession` starts one detector for each accepted remote source stream before playback gain is applied, so muting or lowering volume does not hide remote speaking activity. `ServerMediaAudioSession` reports remote speaking changes through a callback.

`ServerMediaAudioSession` also attempts to enable Opus DTX on local audio senders after the peer connection has a local offer. DTX support is browser-dependent; failure is non-fatal and must not interrupt audio startup.

## Voice Activity Rules

- Sample interval: 40 ms.
- RMS threshold: `0.02`.
- Speaking starts only after at least 100 ms of continuous above-threshold RMS.
- Speaking stops only after at least 650 ms of continuous below-threshold RMS.
- The detector emits only on state transitions.
- Closing a session stops every detector, clears timers, disconnects analyser nodes, and closes the detector-owned `AudioContext`.

The fixed threshold is intentional for this increment. No settings UI or persisted threshold is added.

## UI Behavior

The room user list shows a compact speaking indicator next to each user's nickname. The indicator is active when that user's ID is present in `speakingUserIds`. It is present for the current user and remote users. It must not change the per-user mute or volume semantics.

## Opus DTX Behavior

After creating the WebRTC offer, the frontend inspects local audio `RTCRtpSender`s and calls `setParameters()` with audio encodings updated to request DTX. If no encoding exists, the code creates one encoding object with DTX requested. Because TypeScript DOM types may not expose the non-universal `dtx` field, the implementation may use a narrow local type extension for `RTCRtpEncodingParameters`.

DTX setup is best-effort:

- Unsupported `setParameters()` or rejected updates do not fail `ServerMediaAudioSession.start()`.
- DTX is not used as the active-speaker source of truth.
- The frontend does not pause tracks, call `replaceTrack(null)`, or stop sending audio based on RMS in this increment.

## Testing

Unit tests cover the detector directly with mocked `AudioContext`, `AnalyserNode`, and controlled sample frames:

- RMS above threshold long enough emits `speaking=true`.
- RMS below threshold after speaking emits `speaking=false` only after hangover.
- Short spikes below the start debounce do not emit speaking.
- `stop()` clears the interval and disconnects audio graph nodes.

`ServerMediaAudioSession` tests cover:

- Remote source tracks start per-user VAD and invoke the remote speaking callback.
- Remote VAD is connected before playback gain, so it still analyzes the original remote stream.
- Closing the session stops remote detectors.
- DTX setup calls `setParameters()` on audio senders when supported.
- DTX setup failures do not reject `start()`.

`RoomClient` tests cover:

- A local VAD speaking change marks the current user as active in the list.
- A remote VAD speaking change marks that remote user as active in the list.
- Leaving or closing audio clears speaking state.

## Documentation

Update `docs/roadmap.md` after implementation to note that frontend audio now has client-side per-user active speaker indicators and best-effort Opus DTX.

## Acceptance Criteria

- Per-user speaking indicators appear for local and remote room users.
- Speaking state is computed client-side from RMS energy, without backend speaking messages.
- Remote speaking analysis runs before per-user playback gain.
- Opus DTX is attempted for local audio senders and remains non-fatal.
- Existing server relay audio, mute, volume, diagnostics, and subscription behavior continue to work.
- Frontend tests, typecheck, lint, Rust formatting/lint/tests required by repo guidance, and workspace tests are run or any blocker is explicitly reported.
