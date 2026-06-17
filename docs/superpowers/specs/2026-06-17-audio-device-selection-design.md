# Audio Device Selection Design

## Scope

Add frontend settings controls for microphone and speaker selection. The default for both controls is the browser/system default device. Device choices are browser-local settings persisted through the existing Zustand settings store under `lyre.settings`.

## Requirements

- The settings dialog shows one microphone selector and one speaker selector in the browser audio section.
- Each selector includes a default option:
  - Microphone: `Default microphone`
  - Speaker: `Default speaker`
- When browser media devices are enumerable, audio input devices populate the microphone selector and audio output devices populate the speaker selector.
- If device enumeration is unavailable or rejected, the dialog still renders with the default options and does not block saving other settings.
- The selected microphone device is used when opening local audio by adding a `deviceId: { exact: selectedId }` audio constraint. When the selected input is default, no `deviceId` constraint is sent.
- The selected speaker device is used for remote server-relay playback when the browser supports `AudioContext.setSinkId`. When the selected output is default or `setSinkId` is unavailable, playback uses the browser default output.
- Device changes apply to the next audio session start. Existing connected audio sessions are not restarted, recreated, or retargeted by saving settings that only change microphone or speaker selection.
- If `AudioContext.setSinkId(selectedOutputId)` exists but rejects, the session reports the playback error through the existing error callback and continues with the existing/default playback path.
- Existing browser audio processing settings remain unchanged and continue to combine with the selected microphone constraint.
- No manual direct settings `localStorage` access is added outside the existing Zustand persistence plumbing and test setup.
- No peer mesh audio mode or compatibility fallback is added.

## Acceptance Criteria

- Store tests prove `audioDevices.inputDeviceId` and `audioDevices.outputDeviceId` default to empty strings and persist selected device IDs.
- Settings dialog tests prove the device selectors list enumerated devices and saving persists selected microphone and speaker IDs.
- Settings dialog tests prove rejected or unavailable device enumeration still renders default microphone/speaker options and allows saving other settings.
- Room settings integration tests prove saving only microphone/speaker device changes does not close active audio, recreate local media, restart `ServerMediaAudioSession`, or call `AudioContext.setSinkId` for the active session; selected IDs are used only on the next audio session start.
- WebRTC tests prove `openLocalAudioStream()` sends the stored microphone `deviceId` constraint only when a non-default input is selected.
- Server media audio tests prove remote playback calls `AudioContext.setSinkId(selectedOutputId)` when a speaker is selected and the API exists, does not call it for default output, and reports rejected sink changes without preventing playback setup.
- Frontend targeted tests, lint, and typecheck pass.
- `docs/roadmap.md` is updated with completed audio device selection work and remaining speaker-output browser support caveat.

## Implementation Notes

- Extend `frontend/src/lib/settings-store.ts` with `AudioDeviceConfig`, default empty IDs, a `setAudioDevices` action, and merge hydration defaults.
- Extend `frontend/src/lib/storage.ts` with read/write helpers for audio device config.
- Extend `frontend/src/components/settings-dialog.tsx` to enumerate devices while open and render selectors in the existing browser audio section.
- Extend `frontend/src/lib/webrtc.ts` to read audio devices and include the input device constraint.
- Extend `frontend/src/lib/server-media-audio.ts` to accept an optional `outputDeviceId` and apply it to the playback `AudioContext` when supported.
- Update the room client wiring to pass the stored output device to `ServerMediaAudioSession` when starting audio.
