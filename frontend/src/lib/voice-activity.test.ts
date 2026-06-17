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
