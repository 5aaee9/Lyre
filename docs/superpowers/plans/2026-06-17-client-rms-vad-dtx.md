# Client RMS Voice Activity and Opus DTX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add pure-client per-user active speaker indicators based on RMS audio energy and best-effort Opus DTX for local WebRTC audio senders.

**Architecture:** Create a focused browser voice activity detector module. Wire it into `RoomClient` for local microphone analysis and into `ServerMediaAudioSession` for remote per-source stream analysis before playback gain. Add a best-effort DTX helper inside `ServerMediaAudioSession` that requests Opus DTX without changing connection success semantics.

**Tech Stack:** Next.js, React, TypeScript, Web Audio API, WebRTC, Vitest, Testing Library.

---

## File Structure

- Create `frontend/src/lib/voice-activity.ts`: owns RMS calculation, Web Audio graph setup, debounce/hangover timing, and detector cleanup.
- Create `frontend/src/lib/voice-activity.test.ts`: isolated fake-timer unit tests for detector behavior.
- Modify `frontend/src/lib/server-media-audio.ts`: add remote VAD lifecycle, expose remote speaking callback, and request Opus DTX on audio senders.
- Modify `frontend/src/lib/server-media-audio.test.ts`: extend Web Audio and peer mocks for analyser/DTX coverage.
- Modify `frontend/src/app/room/[roomId]/room-client.tsx`: maintain `speakingUserIds`, start local VAD, receive remote VAD state, render compact indicators, and clear state on audio close.
- Modify `frontend/src/app/room/[roomId]/room-client-test-utils.ts`: extend shared mocks for analyser support and DTX senders.
- Modify `frontend/src/app/room/[roomId]/room-client.test.tsx`: add UI assertions for local and remote speaking indicators.
- Modify `docs/roadmap.md`: document the completed active speaker and DTX increment after implementation review.

## Task 1: Voice Activity Detector Module

**Files:**
- Create: `frontend/src/lib/voice-activity.ts`
- Create: `frontend/src/lib/voice-activity.test.ts`

- [ ] **Step 1: Write the detector tests**

Add `frontend/src/lib/voice-activity.test.ts`:

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { VoiceActivityDetector } from "./voice-activity";

const audioContexts: MockAudioContext[] = [];
const sources: MockAudioSource[] = [];
const analysers: MockAnalyserNode[] = [];
const frames: number[][] = [];

const stream = {} as MediaStream;

class MockAudioSource {
  connect = vi.fn();
  disconnect = vi.fn();

  constructor(readonly input: MediaStream) {
    sources.push(this);
  }
}

class MockAnalyserNode {
  fftSize = 0;
  disconnect = vi.fn();
  getFloatTimeDomainData = vi.fn((data: Float32Array) => {
    const frame = frames.shift() ?? [];
    data.fill(0);
    frame.forEach((sample, index) => {
      data[index] = sample;
    });
  });

  constructor() {
    analysers.push(this);
  }
}

class MockAudioContext {
  createMediaStreamSource = vi.fn((input: MediaStream) => new MockAudioSource(input));
  createAnalyser = vi.fn(() => new MockAnalyserNode());
  close = vi.fn();

  constructor() {
    audioContexts.push(this);
  }
}

function enqueueFrames(sample: number, count: number) {
  for (let index = 0; index < count; index += 1) {
    frames.push(Array.from({ length: 1024 }, () => sample));
  }
}

describe("VoiceActivityDetector", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    audioContexts.length = 0;
    sources.length = 0;
    analysers.length = 0;
    frames.length = 0;
    vi.stubGlobal("AudioContext", MockAudioContext);
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it("emits speaking after sustained RMS above the threshold", async () => {
    const onSpeakingChange = vi.fn();
    const detector = new VoiceActivityDetector(stream, onSpeakingChange);

    detector.start();
    enqueueFrames(0.03, 3);
    await vi.advanceTimersByTimeAsync(120);

    expect(onSpeakingChange).toHaveBeenCalledWith(true);
  });

  it("waits for hangover before emitting not speaking", async () => {
    const onSpeakingChange = vi.fn();
    const detector = new VoiceActivityDetector(stream, onSpeakingChange);

    detector.start();
    enqueueFrames(0.03, 3);
    await vi.advanceTimersByTimeAsync(120);
    enqueueFrames(0, 16);
    await vi.advanceTimersByTimeAsync(640);

    expect(onSpeakingChange).toHaveBeenCalledTimes(1);

    enqueueFrames(0, 1);
    await vi.advanceTimersByTimeAsync(40);

    expect(onSpeakingChange).toHaveBeenLastCalledWith(false);
  });

  it("ignores short spikes below the start debounce", async () => {
    const onSpeakingChange = vi.fn();
    const detector = new VoiceActivityDetector(stream, onSpeakingChange);

    detector.start();
    enqueueFrames(0.03, 2);
    await vi.advanceTimersByTimeAsync(80);
    enqueueFrames(0, 1);
    await vi.advanceTimersByTimeAsync(40);

    expect(onSpeakingChange).not.toHaveBeenCalled();
  });

  it("disconnects graph nodes and closes the audio context on stop", () => {
    const detector = new VoiceActivityDetector(stream, vi.fn());

    detector.start();
    detector.stop();

    expect(sources[0].disconnect).toHaveBeenCalledOnce();
    expect(analysers[0].disconnect).toHaveBeenCalledOnce();
    expect(audioContexts[0].close).toHaveBeenCalledOnce();
  });

  it("does not sample or emit after stop clears the interval", async () => {
    const onSpeakingChange = vi.fn();
    const detector = new VoiceActivityDetector(stream, onSpeakingChange);

    detector.start();
    detector.stop();
    enqueueFrames(0.03, 4);
    await vi.advanceTimersByTimeAsync(160);

    expect(analysers[0].getFloatTimeDomainData).not.toHaveBeenCalled();
    expect(onSpeakingChange).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run the detector test to verify it fails**

Run:

```bash
npm --prefix frontend test -- src/lib/voice-activity.test.ts
```

Expected: fail because `frontend/src/lib/voice-activity.ts` does not exist.

- [ ] **Step 3: Implement the detector**

Create `frontend/src/lib/voice-activity.ts`:

```ts
export type VoiceActivityDetectorOptions = {
  sampleIntervalMs?: number;
  rmsThreshold?: number;
  speakingStartMs?: number;
  speakingStopMs?: number;
};

const DEFAULT_SAMPLE_INTERVAL_MS = 40;
const DEFAULT_RMS_THRESHOLD = 0.02;
const DEFAULT_SPEAKING_START_MS = 100;
const DEFAULT_SPEAKING_STOP_MS = 650;
const ANALYSER_FFT_SIZE = 1024;

export class VoiceActivityDetector {
  private readonly sampleIntervalMs: number;
  private readonly rmsThreshold: number;
  private readonly speakingStartMs: number;
  private readonly speakingStopMs: number;
  private audioContext?: AudioContext;
  private source?: MediaStreamAudioSourceNode;
  private analyser?: AnalyserNode;
  private samples?: Float32Array;
  private interval?: number;
  private speaking = false;
  private aboveThresholdMs = 0;
  private belowThresholdMs = 0;

  constructor(
    private readonly stream: MediaStream,
    private readonly onSpeakingChange: (speaking: boolean) => void,
    options: VoiceActivityDetectorOptions = {}
  ) {
    this.sampleIntervalMs = options.sampleIntervalMs ?? DEFAULT_SAMPLE_INTERVAL_MS;
    this.rmsThreshold = options.rmsThreshold ?? DEFAULT_RMS_THRESHOLD;
    this.speakingStartMs = options.speakingStartMs ?? DEFAULT_SPEAKING_START_MS;
    this.speakingStopMs = options.speakingStopMs ?? DEFAULT_SPEAKING_STOP_MS;
  }

  start(): void {
    if (this.interval !== undefined) {
      return;
    }
    const audioContext = new AudioContext();
    const source = audioContext.createMediaStreamSource(this.stream);
    const analyser = audioContext.createAnalyser();
    analyser.fftSize = ANALYSER_FFT_SIZE;
    source.connect(analyser);
    this.audioContext = audioContext;
    this.source = source;
    this.analyser = analyser;
    this.samples = new Float32Array(analyser.fftSize);
    this.interval = window.setInterval(() => this.sample(), this.sampleIntervalMs);
  }

  stop(): void {
    if (this.interval !== undefined) {
      window.clearInterval(this.interval);
      this.interval = undefined;
    }
    this.source?.disconnect();
    this.analyser?.disconnect();
    void this.audioContext?.close();
    this.audioContext = undefined;
    this.source = undefined;
    this.analyser = undefined;
    this.samples = undefined;
    this.speaking = false;
    this.aboveThresholdMs = 0;
    this.belowThresholdMs = 0;
  }

  private sample(): void {
    if (!this.analyser || !this.samples) {
      return;
    }
    this.analyser.getFloatTimeDomainData(this.samples);
    const rms = calculateRms(this.samples);
    if (rms >= this.rmsThreshold) {
      this.aboveThresholdMs += this.sampleIntervalMs;
      this.belowThresholdMs = 0;
      if (!this.speaking && this.aboveThresholdMs >= this.speakingStartMs) {
        this.speaking = true;
        this.onSpeakingChange(true);
      }
      return;
    }
    this.belowThresholdMs += this.sampleIntervalMs;
    this.aboveThresholdMs = 0;
    if (this.speaking && this.belowThresholdMs >= this.speakingStopMs) {
      this.speaking = false;
      this.onSpeakingChange(false);
    }
  }
}

export function calculateRms(samples: Float32Array): number {
  let sum = 0;
  for (const sample of samples) {
    sum += sample * sample;
  }
  return Math.sqrt(sum / samples.length);
}
```

- [ ] **Step 4: Run the detector test to verify it passes**

Run:

```bash
npm --prefix frontend test -- src/lib/voice-activity.test.ts
```

Expected: pass.

## Task 2: Server Media Remote VAD and Opus DTX

**Files:**
- Modify: `frontend/src/lib/server-media-audio.ts`
- Modify: `frontend/src/lib/server-media-audio.test.ts`

- [ ] **Step 1: Add failing tests for remote VAD and DTX**

Modify `frontend/src/lib/server-media-audio.test.ts`:

1. Import `VoiceActivityDetector`.
2. Mock `./voice-activity`.
3. Add `getSenders`, `setParameters`, and `getParameters` to `MockPeerConnection`.
4. Add these tests:

```ts
const voiceActivityInstances: MockVoiceActivityDetector[] = [];

class MockVoiceActivityDetector {
  start = vi.fn();
  stop = vi.fn();

  constructor(
    readonly stream: MediaStream,
    readonly onSpeakingChange: (speaking: boolean) => void
  ) {
    voiceActivityInstances.push(this);
  }
}

vi.mock("./voice-activity", () => ({
  VoiceActivityDetector: MockVoiceActivityDetector
}));
```

Extend `beforeEach` with:

```ts
voiceActivityInstances.length = 0;
```

Extend `MockPeerConnection` with:

```ts
audioSender = {
  track: { kind: "audio" },
  getParameters: vi.fn(() => ({ encodings: [{}] })),
  setParameters: vi.fn(async () => undefined)
};
getSenders = vi.fn(() => [this.audioSender]);
```

Add tests:

```ts
it("starts remote voice activity detection for accepted source tracks", async () => {
  const onRemoteSpeakingChange = vi.fn();
  const session = new ServerMediaAudioSession({
    roomId: "DEFAULT",
    userId: "user_a",
    accessToken: "token_a",
    socket: { readyState: WebSocket.OPEN, send: socketSend } as unknown as WebSocket,
    iceServers: [{ urls: ["stun:stun.example:3478"], username: null, credential: null }],
    stream,
    pollIntervalMs: 10,
    onRemoteSpeakingChange
  });
  await session.start();

  peerConnections[0].ontrack?.({
    track: { id: "lyre-user:user_b:audio" },
    streams: []
  } as unknown as RTCTrackEvent);

  expect(voiceActivityInstances).toHaveLength(1);
  expect(voiceActivityInstances[0].start).toHaveBeenCalledOnce();

  voiceActivityInstances[0].onSpeakingChange(true);

  expect(onRemoteSpeakingChange).toHaveBeenCalledWith("user_b", true);
});

it("analyzes the original remote stream before playback gain", async () => {
  const session = makeSession();
  await session.start();

  peerConnections[0].ontrack?.({
    track: { id: "lyre-user:user_b:audio" },
    streams: []
  } as unknown as RTCTrackEvent);

  expect(voiceActivityInstances[0].stream).toBe(mediaStreamSources[0].stream);
  expect(mediaStreamSources[0].connect).toHaveBeenCalledWith(gainNodes[0]);
});

it("stops remote voice activity detectors when removing user audio", async () => {
  const session = makeSession();
  await session.start();
  peerConnections[0].ontrack?.({
    track: { id: "lyre-user:user_b:audio" },
    streams: []
  } as unknown as RTCTrackEvent);

  session.removeUserAudio("user_b");

  expect(voiceActivityInstances[0].stop).toHaveBeenCalledOnce();
});

it("requests Opus DTX on local audio senders", async () => {
  const session = makeSession();

  await session.start();

  expect(peerConnections[0].audioSender.setParameters).toHaveBeenCalledWith({
    encodings: [{ dtx: "enabled" }]
  });
});

it("does not fail startup when Opus DTX setup is rejected", async () => {
  peerConnections[0]?.audioSender.setParameters.mockRejectedValueOnce(new Error("unsupported"));
  const session = makeSession();
  peerConnections[0].audioSender.setParameters.mockRejectedValueOnce(new Error("unsupported"));

  await expect(session.start()).resolves.toBeUndefined();
});
```

If the rejected-DTX test needs construction order adjustment, create the session first, mutate `peerConnections[0].audioSender.setParameters`, then call `start()`.

- [ ] **Step 2: Run the targeted test to verify it fails**

Run:

```bash
npm --prefix frontend test -- src/lib/server-media-audio.test.ts
```

Expected: fail because `ServerMediaAudioSession` does not use `VoiceActivityDetector` or DTX.

- [ ] **Step 3: Implement remote VAD lifecycle and DTX**

Modify `frontend/src/lib/server-media-audio.ts`:

- Import `VoiceActivityDetector`.
- Extend `ServerMediaAudioSessionInput` with:

```ts
onRemoteSpeakingChange?: (userId: string, speaking: boolean) => void;
```

- Extend `RemotePlayback` with:

```ts
voiceActivity: VoiceActivityDetector;
```

- After `await this.peer.setLocalDescription(offer);` in `start()`, call:

```ts
await this.enableOpusDtx();
```

- In `addRemoteTrack`, after creating `stream` and before gain setup, create and start:

```ts
const voiceActivity = new VoiceActivityDetector(stream, (speaking) => {
  this.input.onRemoteSpeakingChange?.(sourceUserId, speaking);
});
voiceActivity.start();
```

- Store it in `remotePlayback`.
- In `removeUserAudio`, call `playback.voiceActivity.stop()` before deleting.
- Add helper types and methods:

```ts
type DtxEncodingParameters = RTCRtpEncodingParameters & {
  dtx?: "disabled" | "enabled";
};

type DtxRtpSendParameters = RTCRtpSendParameters & {
  encodings: DtxEncodingParameters[];
};

private async enableOpusDtx(): Promise<void> {
  const senders = this.peer.getSenders().filter((sender) => sender.track?.kind === "audio");
  await Promise.all(senders.map(async (sender) => {
    try {
      const parameters = sender.getParameters() as DtxRtpSendParameters;
      const encodings = parameters.encodings.length > 0 ? parameters.encodings : [{}];
      await sender.setParameters({
        ...parameters,
        encodings: encodings.map((encoding) => ({ ...encoding, dtx: "enabled" }))
      });
    } catch {
      return;
    }
  }));
}
```

- [ ] **Step 4: Run the targeted test to verify it passes**

Run:

```bash
npm --prefix frontend test -- src/lib/server-media-audio.test.ts
```

Expected: pass.

## Task 3: Room UI Speaking State

**Files:**
- Modify: `frontend/src/app/room/[roomId]/room-client.tsx`
- Modify: `frontend/src/app/room/[roomId]/room-client-test-utils.ts`
- Modify: `frontend/src/app/room/[roomId]/room-client.test.tsx`

- [ ] **Step 1: Add failing room UI tests**

In `frontend/src/app/room/[roomId]/room-client-test-utils.ts`, mock `@/lib/voice-activity` similarly to Task 2 and export `voiceActivityInstances`.

In `frontend/src/app/room/[roomId]/room-client.test.tsx`, import `voiceActivityInstances` and add:

```ts
it("shows the current user as speaking from local RMS voice activity", async () => {
  render(<RoomClient roomId="DEFAULT" />);
  await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

  act(() => {
    voiceActivityInstances[0].onSpeakingChange(true);
  });

  expect(screen.getByLabelText("Ada is speaking")).toBeInTheDocument();
});

it("shows a remote user as speaking from remote RMS voice activity", async () => {
  render(<RoomClient roomId="DEFAULT" />);
  await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());
  act(() => {
    peerConnections[0].ontrack?.({
      track: { id: "lyre-user:user_b:audio" },
      streams: []
    } as unknown as RTCTrackEvent);
  });

  act(() => {
    voiceActivityInstances[1].onSpeakingChange(true);
  });

  expect(screen.getByLabelText("Bob is speaking")).toBeInTheDocument();
});

it("clears speaking indicators when audio closes", async () => {
  render(<RoomClient roomId="DEFAULT" />);
  await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());
  act(() => {
    voiceActivityInstances[0].onSpeakingChange(true);
  });

  fireEvent.click(screen.getByText("Leave"));

  await waitFor(() => expect(screen.queryByLabelText("Ada is speaking")).not.toBeInTheDocument());
});
```

- [ ] **Step 2: Run room UI tests to verify they fail**

Run:

```bash
npm --prefix frontend test -- src/app/room/[roomId]/room-client.test.tsx
```

Expected: fail because `RoomClient` does not track or render speaking state.

- [ ] **Step 3: Implement local VAD wiring and speaking indicators**

Modify `frontend/src/app/room/[roomId]/room-client.tsx`:

- Import `VoiceActivityDetector`.
- Add refs/state:

```ts
const localVoiceActivityRef = useRef<VoiceActivityDetector | null>(null);
const [speakingUserIds, setSpeakingUserIds] = useState<Set<string>>(() => new Set());
```

- Add helper callbacks:

```ts
const setUserSpeaking = useCallback((userId: string, speaking: boolean) => {
  setSpeakingUserIds((current) => {
    const next = new Set(current);
    if (speaking) {
      next.add(userId);
    } else {
      next.delete(userId);
    }
    return next;
  });
}, []);

const clearSpeaking = useCallback(() => {
  setSpeakingUserIds(new Set());
}, []);
```

- In `closeAudioSessions`, stop local VAD, null it, and call `clearSpeaking()`.
- In `connectServerRelayAudio`, after opening `stream`, create/start local VAD:
- In `connectServerRelayAudio`, preserve the opened `MediaStream` in the local `stream` variable through `await session.start()` and local VAD startup. The current code sets `stream = null` before `await session.start()`; move that ownership transfer so `stream = null` happens only after both `ServerMediaAudioSession` owns the stream and local VAD has been constructed and started. This ordering prevents a detector leak if media negotiation fails before the session is connected:

```ts
await session.start();
localVoiceActivityRef.current?.stop();
localVoiceActivityRef.current = new VoiceActivityDetector(stream, (speaking) => {
  setUserSpeaking(currentUser.id, speaking);
});
localVoiceActivityRef.current.start();
stream = null;
```

- Pass remote callback to `ServerMediaAudioSession`:

```ts
onRemoteSpeakingChange: setUserSpeaking,
```

- When rendering each user, add an indicator next to nickname:

```tsx
<span className="flex min-w-0 items-center gap-2">
  <span>{user.nickname}</span>
  {speakingUserIds.has(user.id) ? (
    <span
      aria-label={`${user.nickname} is speaking`}
      className="size-2 rounded-full bg-[#2f8f46]"
    />
  ) : null}
</span>
```

- Ensure callbacks are included in dependency arrays.
- Do not call `session.start()` a second time after adding the local VAD. The existing `await session.start()` in this function should move before local VAD startup.

- [ ] **Step 4: Run room UI tests to verify they pass**

Run:

```bash
npm --prefix frontend test -- src/app/room/[roomId]/room-client.test.tsx
```

Expected: pass.

## Task 4: Docs and Full Verification

**Files:**
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Update roadmap**

Add a concise completed item to `docs/roadmap.md`:

```md
- Frontend room audio now shows client-side per-user active speaker indicators from local RMS analysis and requests Opus DTX for local WebRTC audio senders when supported.
```

- [ ] **Step 2: Run frontend targeted tests**

Run:

```bash
npm --prefix frontend test -- src/lib/voice-activity.test.ts src/lib/server-media-audio.test.ts src/app/room/[roomId]/room-client.test.tsx
```

Expected: pass.

- [ ] **Step 3: Run frontend lint and typecheck**

Run:

```bash
npm --prefix frontend run lint
npm --prefix frontend run typecheck
```

Expected: both pass.

- [ ] **Step 4: Run Rust formatting, lint, and workspace tests**

Run:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: pass or report exact blocker if the environment cannot complete them.

- [ ] **Step 5: Inspect final diff**

Run:

```bash
git status --short
git diff -- docs/superpowers/specs/2026-06-17-client-rms-vad-dtx-design.md docs/superpowers/plans/2026-06-17-client-rms-vad-dtx.md frontend/src/lib/voice-activity.ts frontend/src/lib/voice-activity.test.ts frontend/src/lib/server-media-audio.ts frontend/src/lib/server-media-audio.test.ts frontend/src/app/room/[roomId]/room-client.tsx frontend/src/app/room/[roomId]/room-client-test-utils.ts frontend/src/app/room/[roomId]/room-client.test.tsx docs/roadmap.md
```

Expected: only intended files changed.

## Self-Review

- Spec coverage: all acceptance criteria map to Tasks 1-4.
- Placeholder scan: no TBD/TODO/implement-later placeholders remain.
- Type consistency: `VoiceActivityDetector`, `onRemoteSpeakingChange`, and `speakingUserIds` names are consistent across tasks.
