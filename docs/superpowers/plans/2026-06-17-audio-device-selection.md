# Audio Device Selection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add browser-local microphone and speaker selectors to frontend settings, defaulting both to system defaults.

**Architecture:** Persist selected audio device IDs in the existing Zustand settings store. The microphone setting flows into `getUserMedia` constraints, and the speaker setting flows into server-relay remote playback through `AudioContext.setSinkId` when supported.

**Tech Stack:** Next.js, React, Radix/shadcn-style Select, Zustand persist, Vitest, WebRTC/Web Audio browser APIs.

---

## File Structure

- `frontend/src/lib/settings-store.ts`: own persisted `AudioDeviceConfig` and store action.
- `frontend/src/lib/storage.ts`: expose audio device read/write helpers for code that already consumes settings through storage helpers.
- `frontend/src/components/settings-dialog.tsx`: enumerate media devices and render microphone/speaker selectors in the Browser Audio Processing section.
- `frontend/src/lib/webrtc.ts`: include stored microphone `deviceId` constraint for non-default input.
- `frontend/src/lib/server-media-audio.ts`: accept an optional output device ID and apply it to the playback `AudioContext` when supported.
- `frontend/src/app/room/[roomId]/room-client.tsx`: pass stored output device ID into `ServerMediaAudioSession`.
- Tests:
  - `frontend/src/lib/settings-store.test.ts`
  - `frontend/src/components/settings-dialog.test.tsx`
  - `frontend/src/lib/webrtc.test.ts`
  - `frontend/src/lib/server-media-audio.test.ts`
  - `frontend/src/app/room/[roomId]/room-client-settings.test.tsx`
- Docs:
  - `docs/roadmap.md`

## Task 0: Inspect Existing Partial Work

**Files:**
- Inspect: `frontend/src/lib/settings-store.ts`
- Inspect: `frontend/src/lib/storage.ts`
- Inspect: `frontend/src/lib/settings-store.test.ts`
- Inspect: `frontend/src/lib/webrtc.test.ts`
- Inspect: `frontend/src/lib/server-media-audio.test.ts`
- Inspect: `frontend/src/components/settings-dialog.test.tsx`

- [ ] **Step 1: Check current worktree**

Run:

```bash
git status --short
git diff -- frontend/src/lib/settings-store.ts frontend/src/lib/storage.ts frontend/src/lib/settings-store.test.ts frontend/src/lib/webrtc.test.ts frontend/src/lib/server-media-audio.test.ts frontend/src/components/settings-dialog.test.tsx
```

Expected: the worktree may already contain partial audio-device store and failing-test edits from the interrupted session. Preserve those edits and continue from them; do not revert them.

- [ ] **Step 2: Run current targeted tests**

Run:

```bash
npm --prefix frontend test -- --run src/lib/settings-store.test.ts src/lib/webrtc.test.ts src/lib/server-media-audio.test.ts src/components/settings-dialog.test.tsx
```

Expected: existing red tests identify remaining gaps. At the time this plan was written, known failures were missing settings dialog device selectors, missing microphone `deviceId` constraint, and missing speaker `setSinkId` handling.

## Task 1: Persist Audio Device Settings

**Files:**
- Modify: `frontend/src/lib/settings-store.ts`
- Modify: `frontend/src/lib/storage.ts`
- Test: `frontend/src/lib/settings-store.test.ts`

- [ ] **Step 1: Write failing store tests**

Add assertions that `readSettingsSnapshot()` includes:

```ts
audioDevices: {
  inputDeviceId: "",
  outputDeviceId: ""
}
```

In the persistence test, call:

```ts
useSettingsStore.getState().setAudioDevices({
  inputDeviceId: "mic-a",
  outputDeviceId: "speaker-a"
});
```

and assert local storage contains:

```ts
audioDevices: {
  inputDeviceId: "mic-a",
  outputDeviceId: "speaker-a"
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
npm --prefix frontend test -- --run src/lib/settings-store.test.ts
```

Expected: FAIL if the interrupted partial store implementation is absent; otherwise this may already PASS and the worker should keep the existing implementation.

- [ ] **Step 3: Implement store and helper support**

In `settings-store.ts`, add:

```ts
export type AudioDeviceConfig = {
  inputDeviceId: string;
  outputDeviceId: string;
};

export const defaultAudioDeviceConfig: AudioDeviceConfig = {
  inputDeviceId: "",
  outputDeviceId: ""
};
```

Add `audioDevices`, `setAudioDevices`, merge hydration defaults, and default state entry.

In `storage.ts`, add:

```ts
export function readAudioDeviceConfig(): AudioDeviceConfig {
  return {
    ...defaultAudioDeviceConfig,
    ...readSettingsSnapshot().audioDevices
  };
}

export function writeAudioDeviceConfig(config: AudioDeviceConfig): void {
  useSettingsStore.getState().setAudioDevices(config);
}
```

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
npm --prefix frontend test -- --run src/lib/settings-store.test.ts
```

Expected: PASS.

## Task 2: Add Device Selectors To Settings Dialog

**Files:**
- Modify: `frontend/src/components/settings-dialog.tsx`
- Test: `frontend/src/components/settings-dialog.test.tsx`

- [ ] **Step 1: Write failing dialog test**

Stub `navigator.mediaDevices.enumerateDevices()` with one `audioinput` and one `audiooutput`, then in the existing save test select:

```ts
await chooseSelectOption("Microphone", "Studio Mic");
await chooseSelectOption("Speaker", "Desk Speakers");
```

Assert `readSettingsSnapshot().audioDevices` equals:

```ts
{
  inputDeviceId: "mic-a",
  outputDeviceId: "speaker-a"
}
```

Add a second test where `enumerateDevices` rejects. Assert the dialog still renders `Default microphone` and `Default speaker`, saving another setting such as nickname still closes the dialog, and `audioDevices` remains:

```ts
{
  inputDeviceId: "",
  outputDeviceId: ""
}
```

Add a third test where `navigator.mediaDevices` is unavailable or has no `enumerateDevices`. Assert the same default microphone/speaker options render and saving another setting still works.

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
npm --prefix frontend test -- --run src/components/settings-dialog.test.tsx
```

Expected: FAIL if the settings dialog selectors are still absent.

- [ ] **Step 3: Implement device enumeration and selectors**

In `settings-dialog.tsx`:

- Import `useEffect`.
- Read `audioDevices` and `setAudioDevices` from `useSettingsStore`.
- Keep local `mediaDevices: MediaDeviceInfo[]` state.
- When `open` is true, call `navigator.mediaDevices?.enumerateDevices?.()`, set devices on success, and set `[]` on rejection.
- Render two selects in the Browser Audio Processing section:
  - `aria-label="Microphone"`, default item value `"default"` displayed as `Default microphone`, plus `audioinput` devices.
  - `aria-label="Speaker"`, default item value `"default"` displayed as `Default speaker`, plus `audiooutput` devices.
- Translate between persisted empty string and select value `"default"` because Radix Select item values cannot be empty strings.

- [ ] **Step 4: Run dialog test**

Run:

```bash
npm --prefix frontend test -- --run src/components/settings-dialog.test.tsx
```

Expected: PASS.

## Task 3: Apply Microphone Device Constraint

**Files:**
- Modify: `frontend/src/lib/webrtc.ts`
- Test: `frontend/src/lib/webrtc.test.ts`

- [ ] **Step 1: Write failing WebRTC tests**

Add tests proving:

- selected `inputDeviceId: "mic-a"` produces `deviceId: { exact: "mic-a" }`;
- default `inputDeviceId: ""` omits `deviceId`.

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
npm --prefix frontend test -- --run src/lib/webrtc.test.ts
```

Expected: FAIL before implementation because `deviceId` is not included for selected input.

- [ ] **Step 3: Implement constraint**

In `webrtc.ts`, read `readAudioDeviceConfig()`, construct the audio constraint object, and add:

```ts
...(audioDevices.inputDeviceId
  ? { deviceId: { exact: audioDevices.inputDeviceId } }
  : {})
```

Keep existing echo cancellation, auto gain control, and noise suppression constraints unchanged.

- [ ] **Step 4: Run WebRTC tests**

Run:

```bash
npm --prefix frontend test -- --run src/lib/webrtc.test.ts
```

Expected: PASS.

## Task 4: Apply Speaker Output Device To Server Playback

**Files:**
- Modify: `frontend/src/lib/server-media-audio.ts`
- Modify: `frontend/src/app/room/[roomId]/room-client.tsx`
- Test: `frontend/src/lib/server-media-audio.test.ts`
- Test: `frontend/src/app/room/[roomId]/room-client-settings.test.tsx`

- [ ] **Step 1: Write failing server media tests**

Add tests proving:

- constructing `ServerMediaAudioSession` with `outputDeviceId: "speaker-a"` calls `AudioContext.setSinkId("speaker-a")` when a valid remote track starts;
- default output does not call `setSinkId`.
- rejected `setSinkId("speaker-a")` reports the error through `onError` and still connects the remote track through Web Audio gain.

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
npm --prefix frontend test -- --run src/lib/server-media-audio.test.ts
```

Expected: FAIL before implementation because `outputDeviceId` is not accepted or applied.

- [ ] **Step 3: Implement output routing**

In `ServerMediaAudioSessionInput`, add:

```ts
outputDeviceId?: string;
```

Add a small local type guard/helper for `AudioContext.setSinkId` support:

```ts
type AudioContextWithSinkId = AudioContext & {
  setSinkId?: (sinkId: string) => Promise<void>;
};
```

When creating/reusing the audio context for remote playback, if `input.outputDeviceId` is non-empty and `setSinkId` exists, call it and report errors through `reportPlaybackError`. Do not abort remote playback setup if the promise rejects.

In `room-client.tsx`, read `audioDevices` from the settings store and pass `outputDeviceId: audioDevices.outputDeviceId` when creating `ServerMediaAudioSession`.

In `room-client.tsx`, maintain explicit applied-setting baselines because `SettingsDialog` mutates Zustand before calling `onSave`:

- `appliedNoiseRef` initialized from `readNoiseConfig()`.
- `appliedAudioProcessingRef` initialized from `readAudioProcessingConfig()`.
- After a successful `connectServerRelayAudio()` start/reconnect, update both refs to the noise/audio-processing values used for that started session.
- In `saveSettings(settings)`, compare `settings.noise` to `appliedNoiseRef.current` and `settings.audioProcessing` to `appliedAudioProcessingRef.current`. If both are equal, return without calling `closeAudioSessions()`, `updateMediaRelaySettings()`, `connectServerRelayAudio()`, or active-session sink retargeting. This is the device-only save path.
- If either restart-worthy config differs, keep the existing reconnect/update behavior and update refs only after the reconnect path succeeds.

Add a second room-client settings test that first changes server noise and observes the existing reconnect, then opens settings again, changes only microphone/speaker device IDs, saves, and asserts no third `answerServerMediaOffer`, no additional `getUserMedia`, and no additional peer close/track stop occur. This proves the baseline advances after a successful noise update.

- [ ] **Step 4: Run server media tests**

Run:

```bash
npm --prefix frontend test -- --run src/lib/server-media-audio.test.ts
```

Expected: PASS.

- [ ] **Step 5: Add and run room settings integration test**

In `room-client-settings.test.tsx`, add a test that starts the room audio through the existing setup, opens Settings, changes only Microphone and Speaker, saves, and asserts existing active audio is not restarted:

```ts
expect(openLocalAudioStream).toHaveBeenCalledTimes(1);
expect(closeServerMediaSession).not.toHaveBeenCalled();
expect(serverMediaAudioSessionStart).toHaveBeenCalledTimes(1);
```

Use the existing mocks and helper patterns in that test file; if exact mock names differ, assert the same behavior through the local mocks already used there. Also assert that when a new `ServerMediaAudioSession` is constructed on the next audio start/reconnect path, it receives `outputDeviceId: "speaker-a"`.

Run:

```bash
npm --prefix frontend test -- --run src/app/room/[roomId]/room-client-settings.test.tsx
```

Expected: PASS.

## Task 5: Full Verification And Docs

**Files:**
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Run targeted frontend tests**

Run:

```bash
npm --prefix frontend test -- --run src/lib/settings-store.test.ts src/lib/webrtc.test.ts src/lib/server-media-audio.test.ts src/components/settings-dialog.test.tsx src/app/room/[roomId]/room-client-settings.test.tsx
```

Expected: PASS.

- [ ] **Step 2: Run frontend lint and typecheck**

Run:

```bash
npm --prefix frontend run lint
npm --prefix frontend run typecheck
```

Expected: both exit 0.

- [ ] **Step 3: Update roadmap**

Add an entry to `docs/roadmap.md` noting completed frontend microphone/speaker selection and any remaining browser support caveat for speaker output APIs.

- [ ] **Step 4: Review diff**

Run:

```bash
git diff -- frontend/src/lib/settings-store.ts frontend/src/lib/storage.ts frontend/src/components/settings-dialog.tsx frontend/src/lib/webrtc.ts frontend/src/lib/server-media-audio.ts frontend/src/app/room/[roomId]/room-client.tsx frontend/src/lib/settings-store.test.ts frontend/src/components/settings-dialog.test.tsx frontend/src/lib/webrtc.test.ts frontend/src/lib/server-media-audio.test.ts docs/roadmap.md docs/superpowers/specs/2026-06-17-audio-device-selection-design.md docs/superpowers/plans/2026-06-17-audio-device-selection.md
```

Expected: only focused audio device selection changes.
