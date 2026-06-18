import { beforeEach, describe, expect, it, vi } from "vitest";
import { processLocalAudioStream } from "./client-noise-cancellation";

const wasmLoader = vi.hoisted(() => ({
  loadLyreNoiseWasm: vi.fn()
}));

vi.mock("./lyre-noise-wasm", () => ({
  loadLyreNoiseWasm: wasmLoader.loadLyreNoiseWasm
}));

const inputTrackStop = vi.fn();
const outputTrackStop = vi.fn();
const sourceDisconnect = vi.fn();
const processorDisconnect = vi.fn();
const contextClose = vi.fn();
const addModule = vi.fn();
const sourceConnect = vi.fn();
const processorConnect = vi.fn();
const workletNodes: MockAudioWorkletNode[] = [];
const outputTrack = {
  id: "processed",
  stop: outputTrackStop
};
const destinationStream = {
  getAudioTracks: () => [outputTrack]
} as unknown as MediaStream;
const inputStream = {
  getAudioTracks: () => [{
    id: "raw",
    stop: inputTrackStop
  }]
} as unknown as MediaStream;

class MockAudioContext {
  sampleRate = 48_000;
  audioWorklet = {
    addModule
  };
  createMediaStreamSource = vi.fn(() => ({
    connect: sourceConnect,
    disconnect: sourceDisconnect
  }));
  createMediaStreamDestination = vi.fn(() => ({
    stream: destinationStream
  }));
  close = contextClose;

  constructor(options: AudioContextOptions) {
    this.sampleRate = options.sampleRate ?? 48_000;
  }
}

class MockAudioWorkletNode {
  port: {
    onmessage: ((event: MessageEvent) => void) | null;
  } = {
    onmessage: null
  };
  connect = processorConnect;
  disconnect = processorDisconnect;

  constructor(
    readonly context: AudioContext,
    readonly name: string,
    readonly options: AudioWorkletNodeOptions
  ) {
    workletNodes.push(this);
    queueMicrotask(() => {
      this.port.onmessage?.({ data: { type: "ready" } } as MessageEvent);
    });
  }
}

describe("client noise cancellation", () => {
  beforeEach(() => {
    wasmLoader.loadLyreNoiseWasm.mockReset();
    wasmLoader.loadLyreNoiseWasm.mockResolvedValue({
      bytes: new ArrayBuffer(8),
      channels: 1,
      frameSize: 480,
      sampleRateHz: 48_000
    });
    workletNodes.length = 0;
    inputTrackStop.mockClear();
    outputTrackStop.mockClear();
    sourceDisconnect.mockClear();
    processorDisconnect.mockClear();
    processorConnect.mockClear();
    sourceConnect.mockClear();
    contextClose.mockClear();
    addModule.mockClear();
    outputTrack.stop = outputTrackStop;
    vi.stubGlobal("AudioContext", MockAudioContext);
    vi.stubGlobal("AudioWorkletNode", MockAudioWorkletNode);
  });

  it("routes microphone audio through the Lyre noise worklet", async () => {
    await expect(processLocalAudioStream(inputStream)).resolves.toBe(destinationStream);

    expect(addModule).toHaveBeenCalledWith("/audio-worklets/lyre-noise-processor.js");
    expect(workletNodes[0].name).toBe("lyre-noise-processor");
    expect(workletNodes[0].options).toMatchObject({
      numberOfInputs: 1,
      numberOfOutputs: 1,
      outputChannelCount: [1],
      processorOptions: {
        frameSize: 480
      }
    });
    expect(sourceConnect).toHaveBeenCalledWith(workletNodes[0]);
    expect(processorConnect).toHaveBeenCalledOnce();
  });

  it("stops the raw track and closes the graph when the processed track stops", async () => {
    const output = await processLocalAudioStream(inputStream);

    output.getAudioTracks()[0].stop();

    expect(outputTrackStop).toHaveBeenCalledOnce();
    expect(inputTrackStop).toHaveBeenCalledOnce();
    expect(sourceDisconnect).toHaveBeenCalledOnce();
    expect(processorDisconnect).toHaveBeenCalledOnce();
    expect(contextClose).toHaveBeenCalledOnce();
  });
});
