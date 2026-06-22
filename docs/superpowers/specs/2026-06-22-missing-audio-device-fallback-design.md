# Missing Audio Device Fallback Design

## Scope

When the browser reports that a previously saved microphone device cannot be found, Lyre should fall back to the browser default microphone instead of leaving the room audio startup stuck on `Requested device not found`.

This applies only to frontend microphone capture through `frontend/src/lib/webrtc.ts`. It does not change speaker routing, server relay behavior, permission handling, noise cancellation, or browser DSP settings.

## Requirements

- `openLocalAudioStream()` keeps using a saved non-default `audioDevices.inputDeviceId` as an exact `getUserMedia` `deviceId` constraint.
- If that first `getUserMedia` call fails because the selected input device is missing, `openLocalAudioStream()` clears only `audioDevices.inputDeviceId` in the existing Zustand-backed settings store.
- Missing selected input errors include `NotFoundError` and `OverconstrainedError` only when the failed constraint is `deviceId`.
- After clearing the stale input ID, `openLocalAudioStream()` retries once with the same audio processing constraints and no `deviceId` constraint so the browser default microphone can be used.
- The stored `audioDevices.outputDeviceId` is preserved when clearing the stale input ID.
- If no input device is saved, or if the failure is not a missing-device error, the original error is still thrown.
- Client-side noise cancellation behavior remains unchanged and wraps the stream only after a microphone stream is opened.
- No direct `localStorage` reads or writes are added; settings changes use the existing store/storage helper path.
- No peer mesh audio mode or audio topology fallback is added.

## Acceptance Criteria

- WebRTC unit tests prove saved missing input device failures first call `getUserMedia` with `deviceId: { exact: savedId }`, then retry without `deviceId`, resolve with the retry stream, clear `audioDevices.inputDeviceId`, and preserve `audioDevices.outputDeviceId` for both `NotFoundError` and `OverconstrainedError` on the `deviceId` constraint.
- Existing WebRTC unit tests still prove selected valid microphones use exact `deviceId` and default input omits `deviceId`.
- Existing audio processing and client-side noise cancellation tests continue to pass.
- Frontend typecheck, lint, and targeted/full frontend tests pass.
- Repository Rust verification required by the project still passes even though the implementation is frontend-only.
- `docs/roadmap.md` records the completed fallback behavior.

## Implementation Notes

- Add a small local helper in `frontend/src/lib/webrtc.ts` for building the local audio constraints so the initial call and fallback retry cannot drift.
- Treat browser missing-device errors by checking `error.name === "NotFoundError"` on `DOMException` or `Error` values, matching the observed `NotFoundError: Requested device not found` path.
- Also treat `error.name === "OverconstrainedError"` as a missing selected input only when `error.constraint === "deviceId"`, because exact device constraints can fail after the browser has no matching candidate device.
- Keep the fallback narrow. Do not retry permission denial, generic `AbortError`, audio processing constraint errors, or other media failures.
