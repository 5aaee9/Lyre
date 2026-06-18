import { beforeEach, describe, expect, it, vi } from "vitest";
import { processLocalAudioStream } from "./client-noise-cancellation";
import { defaultNoiseConfig } from "./settings-store";

const wasmLoader = vi.hoisted(() => ({
  loadCachedNoiseModelBundle: vi.fn(),
  loadLyreNoiseWasm: vi.fn()
}));

vi.mock("./noise-model-cache", () => ({
  loadCachedNoiseModelBundle: wasmLoader.loadCachedNoiseModelBundle
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
const workerPostMessage = vi.fn();
const workerTerminate = vi.fn();
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
    postMessage: ReturnType<typeof vi.fn>;
  } = {
    onmessage: null,
    postMessage: vi.fn()
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

class MockWorker {
  onmessage: ((event: MessageEvent) => void) | null = null;
  postMessage = workerPostMessage;
  terminate = workerTerminate;

  constructor() {
    queueMicrotask(() => {
      this.onmessage?.({ data: { type: "ready" } } as MessageEvent);
    });
  }
}

describe("client noise cancellation", () => {
  beforeEach(() => {
    wasmLoader.loadLyreNoiseWasm.mockReset();
    wasmLoader.loadCachedNoiseModelBundle.mockReset();
    wasmLoader.loadLyreNoiseWasm.mockResolvedValue({
      bytes: new ArrayBuffer(8),
      channels: 1,
      frameSize: 480,
      sampleRateHz: 48_000
    });
    wasmLoader.loadCachedNoiseModelBundle.mockResolvedValue(null);
    workletNodes.length = 0;
    inputTrackStop.mockClear();
    outputTrackStop.mockClear();
    sourceDisconnect.mockClear();
    processorDisconnect.mockClear();
    processorConnect.mockClear();
    sourceConnect.mockClear();
    contextClose.mockClear();
    addModule.mockClear();
    workerPostMessage.mockClear();
    workerTerminate.mockClear();
    outputTrack.stop = outputTrackStop;
    vi.stubGlobal("AudioContext", MockAudioContext);
    vi.stubGlobal("AudioWorkletNode", MockAudioWorkletNode);
    vi.stubGlobal("Worker", MockWorker);
  });

  it("routes microphone audio through the Lyre noise worklet", async () => {
    await expect(processLocalAudioStream(inputStream, { noise: defaultNoiseConfig })).resolves.toBe(destinationStream);

    expect(addModule).toHaveBeenCalledWith("/audio-worklets/lyre-noise-processor.js");
    expect(workletNodes[0].name).toBe("lyre-noise-processor");
    expect(workletNodes[0].options).toMatchObject({
      numberOfInputs: 1,
      numberOfOutputs: 1,
      outputChannelCount: [1],
      processorOptions: {
        frameSize: 480,
        model: "dpdfnet2_48khz_hr",
        processor: "dpdfnet"
      }
    });
    expect(wasmLoader.loadCachedNoiseModelBundle).toHaveBeenCalledWith(defaultNoiseConfig);
    expect(sourceConnect).toHaveBeenCalledWith(workletNodes[0]);
    expect(processorConnect).toHaveBeenCalledOnce();
  });

  it("stops the raw track and closes the graph when the processed track stops", async () => {
    const output = await processLocalAudioStream(inputStream, { noise: defaultNoiseConfig });

    output.getAudioTracks()[0].stop();

    expect(outputTrackStop).toHaveBeenCalledOnce();
    expect(inputTrackStop).toHaveBeenCalledOnce();
    expect(sourceDisconnect).toHaveBeenCalledOnce();
    expect(processorDisconnect).toHaveBeenCalledOnce();
    expect(contextClose).toHaveBeenCalledOnce();
  });

  it("uses RNNoise in the worklet for non-ONNX client providers", async () => {
    await expect(
      processLocalAudioStream(inputStream, {
        noise: {
          ...defaultNoiseConfig,
          provider: "rnnoise"
        }
      })
    ).resolves.toBe(destinationStream);

    expect(workletNodes[0].options.processorOptions).toMatchObject({
      processor: "rnnoise"
    });
  });

  it("passes cached ONNX models into the worklet", async () => {
    const modelBundle = {
      provider: "deepfilternet",
      models: [
        {
          bytes: new ArrayBuffer(4),
          name: "enc",
          url: "/models/deepfilternet/enc.onnx"
        }
      ]
    };
    wasmLoader.loadCachedNoiseModelBundle.mockResolvedValue(modelBundle);

    await expect(
      processLocalAudioStream(inputStream, {
        noise: {
          ...defaultNoiseConfig,
          provider: "deepfilternet"
        }
      })
    ).resolves.toBe(destinationStream);

    expect(workletNodes[0].options.processorOptions).toMatchObject({
      hasOnnxModels: true,
      processor: "deepfilternet"
    });
    expect(workerPostMessage).toHaveBeenCalledWith(
      expect.objectContaining({
        provider: "deepfilternet",
        type: "init"
      }),
      expect.any(Array)
    );
    expect(workletNodes[0].port.postMessage).toHaveBeenCalledWith(
      expect.objectContaining({ type: "onnx-port" }),
      expect.any(Array)
    );
  });

  it("passes the cached DPDFNet manifest into the ONNX worker", async () => {
    const modelBundle = {
      provider: "dpdfnet",
      manifest: {
        initialState: [0.25, 0.5]
      },
      models: [
        {
          bytes: new ArrayBuffer(4),
          name: "dpdfnet8_48khz_hr",
          url: "/models/dpdfnet/dpdfnet8_48khz_hr.onnx"
        }
      ]
    };
    wasmLoader.loadCachedNoiseModelBundle.mockResolvedValue(modelBundle);

    await expect(
      processLocalAudioStream(inputStream, {
        noise: {
          ...defaultNoiseConfig,
          provider: "dpdfnet",
          dpdfnet: {
            model: "dpdfnet8_48khz_hr"
          }
        }
      })
    ).resolves.toBe(destinationStream);

    expect(workerPostMessage).toHaveBeenCalledWith(
      expect.objectContaining({
        manifest: {
          initialState: [0.25, 0.5]
        },
        model: "dpdfnet8_48khz_hr",
        provider: "dpdfnet",
        type: "init"
      }),
      expect.any(Array)
    );
  });
});
