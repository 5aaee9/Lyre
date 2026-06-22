# Missing Audio Device Fallback Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make frontend microphone capture recover from a stale saved microphone ID by retrying with the browser default microphone.

**Architecture:** Keep `openLocalAudioStream()` as the single microphone capture boundary. Build local audio constraints through a helper, catch only missing-device errors for saved input IDs, clear the stale input through the existing settings helper, then retry once without a `deviceId` constraint. Missing-device errors are `NotFoundError` and `OverconstrainedError` limited to the `deviceId` constraint.

**Tech Stack:** Next.js frontend, TypeScript, WebRTC `getUserMedia`, Zustand settings store, Vitest.

---

## File Structure

- `frontend/src/lib/webrtc.ts`: implement the narrow missing-device fallback and shared audio constraint builder.
- `frontend/src/lib/webrtc.test.ts`: add the regression test for stale saved microphone fallback.
- `docs/roadmap.md`: record the completed client fallback behavior.

## Task 1: Add Stale Microphone Fallback

**Files:**
- Modify: `frontend/src/lib/webrtc.test.ts`
- Modify: `frontend/src/lib/webrtc.ts`
- Modify: `docs/roadmap.md`

- [x] **Step 1: Write the failing WebRTC regression tests**

In `frontend/src/lib/webrtc.test.ts`, after the existing `"uses the stored microphone device when opening local audio"` test, add:

```ts
  it("retries with the default microphone when the stored input device is missing", async () => {
    useSettingsStore.getState().setAudioDevices({
      inputDeviceId: "missing-mic",
      outputDeviceId: "speaker-a"
    });
    const getUserMedia = vi.mocked(navigator.mediaDevices.getUserMedia);
    getUserMedia
      .mockRejectedValueOnce(Object.assign(new Error("Requested device not found"), { name: "NotFoundError" }))
      .mockResolvedValueOnce(stream);

    await expect(openLocalAudioStream()).resolves.toBe(stream);

    expect(getUserMedia).toHaveBeenNthCalledWith(1, {
      audio: {
        deviceId: { exact: "missing-mic" },
        echoCancellation: true,
        autoGainControl: true,
        noiseSuppression: true
      }
    });
    expect(getUserMedia).toHaveBeenNthCalledWith(2, {
      audio: {
        echoCancellation: true,
        autoGainControl: true,
        noiseSuppression: true
      }
    });
    expect(useSettingsStore.getState().audioDevices).toEqual({
      inputDeviceId: "",
      outputDeviceId: "speaker-a"
    });
  });

  it("retries with the default microphone when the exact input device constraint cannot be satisfied", async () => {
    useSettingsStore.getState().setAudioDevices({
      inputDeviceId: "missing-mic",
      outputDeviceId: "speaker-a"
    });
    const getUserMedia = vi.mocked(navigator.mediaDevices.getUserMedia);
    getUserMedia
      .mockRejectedValueOnce(Object.assign(new Error("Requested device not found"), {
        constraint: "deviceId",
        name: "OverconstrainedError"
      }))
      .mockResolvedValueOnce(stream);

    await expect(openLocalAudioStream()).resolves.toBe(stream);

    expect(getUserMedia).toHaveBeenNthCalledWith(1, {
      audio: {
        deviceId: { exact: "missing-mic" },
        echoCancellation: true,
        autoGainControl: true,
        noiseSuppression: true
      }
    });
    expect(getUserMedia).toHaveBeenNthCalledWith(2, {
      audio: {
        echoCancellation: true,
        autoGainControl: true,
        noiseSuppression: true
      }
    });
    expect(useSettingsStore.getState().audioDevices).toEqual({
      inputDeviceId: "",
      outputDeviceId: "speaker-a"
    });
  });
```

- [x] **Step 2: Run the focused test and verify it fails**

Run:

```bash
pnpm test src/lib/webrtc.test.ts --run
```

from `/home/indexyz/lyre/frontend`.

Expected: the new tests fail before implementation because `openLocalAudioStream()` rejects with missing-device errors instead of retrying.

- [x] **Step 3: Implement the fallback**

In `frontend/src/lib/webrtc.ts`, import `writeAudioDeviceConfig` from `./storage` and `AudioProcessingConfig` from `./settings-store`.

Replace the direct `getUserMedia` call in `openLocalAudioStream()` with:

```ts
  let stream: MediaStream;
  try {
    stream = await navigator.mediaDevices.getUserMedia(localAudioConstraints(audioProcessing, audioDevices.inputDeviceId));
  } catch (error) {
    if (!audioDevices.inputDeviceId || !isMissingAudioDeviceError(error)) {
      throw error;
    }
    writeAudioDeviceConfig({ ...audioDevices, inputDeviceId: "" });
    stream = await navigator.mediaDevices.getUserMedia(localAudioConstraints(audioProcessing, ""));
  }
```

Add these helpers below `openLocalAudioStream()`:

```ts
function localAudioConstraints(audioProcessing: AudioProcessingConfig, inputDeviceId: string): MediaStreamConstraints {
  return {
    audio: {
      ...(inputDeviceId ? { deviceId: { exact: inputDeviceId } } : {}),
      echoCancellation: audioConstraint(audioProcessing.echoCancellation),
      autoGainControl: audioConstraint(audioProcessing.autoGainControl),
      noiseSuppression: audioConstraint(audioProcessing.noiseSuppression)
    }
  };
}

function isMissingAudioDeviceError(error: unknown): boolean {
  if (!(error instanceof Error)) {
    return false;
  }
  return error.name === "NotFoundError" || (
    error.name === "OverconstrainedError" &&
    "constraint" in error &&
    error.constraint === "deviceId"
  );
}
```

Keep the existing client-side noise cancellation block after stream acquisition.

- [x] **Step 4: Run the focused test and verify it passes**

Run:

```bash
pnpm test src/lib/webrtc.test.ts --run
```

from `/home/indexyz/lyre/frontend`.

Expected: `src/lib/webrtc.test.ts` passes, including both stale saved microphone fallback tests.

- [x] **Step 5: Update the roadmap**

Add this bullet under `## Completed` in `docs/roadmap.md`:

```md
- Frontend microphone capture now clears a stale saved input device and retries with the browser default microphone when `getUserMedia` reports the selected device is missing.
```

- [x] **Step 6: Run full verification**

Run:

```bash
pnpm test --run
pnpm typecheck
pnpm lint
cargo fmt -- --check
cargo clippy
cargo nextest run --manifest-path "Cargo.toml" --workspace
git diff --check
```

Expected: all commands exit 0.

## Self-Review

- Spec coverage: Task 1 covers the WebRTC regression tests, fallback implementation, store update through existing helper, output device preservation, retry with default microphone, unchanged client noise cancellation ordering, roadmap update, and required verification.
- Placeholder scan: no deferred implementation language remains.
- Type consistency: helper names, storage imports, and settings types match existing code paths in `frontend/src/lib/webrtc.ts`.
