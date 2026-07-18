import { vi } from "vitest";

export type NoiseLoaderMocks = {
  loadCachedNoiseModelBundle: ReturnType<typeof vi.fn>;
  loadLyreNoiseWasm: ReturnType<typeof vi.fn>;
};

type StartupMessage = { message?: string; type: "error" | "ready" };

export const inputTrackStop = vi.fn();
export const outputTrackStop = vi.fn();
export const secondOutputTrackStop = vi.fn();
export const sourceDisconnect = vi.fn();
export const processorDisconnect = vi.fn();
export const contextClose = vi.fn();
export const addModule = vi.fn();
export const workerPostMessage = vi.fn();
export const workerTerminate = vi.fn();
export const sourceConnect = vi.fn();
export const processorConnect = vi.fn();

let workerReadyMessage: StartupMessage | null = { type: "ready" };
let workletReadyMessage: StartupMessage | null = { type: "ready" };
let workerStartupError: Error | null = null;
let workerStartupMessageError: Error | null = null;
let workletStartupProcessorError: Error | null = null;
let workletStartupMessageError: Error | null = null;

export const outputTrack = {
  id: "processed",
  stop: outputTrackStop
};
export const secondOutputTrack = {
  id: "processed-2",
  stop: secondOutputTrackStop
};
let destinationAudioTracks = [outputTrack];

export const destinationStream = {
  getAudioTracks: () => destinationAudioTracks
} as unknown as MediaStream;

export const inputStream = {
  getAudioTracks: () => [{
    id: "raw",
    stop: inputTrackStop
  }]
} as unknown as MediaStream;

class MockAudioContext {
  sampleRate = 48_000;
  audioWorklet = { addModule };
  createMediaStreamSource = vi.fn(() => ({
    connect: sourceConnect,
    disconnect: sourceDisconnect
  }));
  createMediaStreamDestination = vi.fn(() => ({ stream: destinationStream }));
  close = contextClose;

  constructor(options: AudioContextOptions) {
    this.sampleRate = options.sampleRate ?? 48_000;
  }
}

export class MockAudioWorkletNode {
  onprocessorerror: ((event: Event) => void) | null = null;
  port: {
    onmessage: ((event: MessageEvent) => void) | null;
    onmessageerror: ((event: MessageEvent) => void) | null;
    postMessage: ReturnType<typeof vi.fn>;
  } = {
    onmessage: null,
    onmessageerror: null,
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
      if (workletStartupProcessorError) {
        this.onprocessorerror?.({
          error: workletStartupProcessorError,
          message: workletStartupProcessorError.message
        } as ErrorEvent);
        return;
      }
      if (workletStartupMessageError) {
        this.port.onmessageerror?.({ data: workletStartupMessageError } as MessageEvent);
        return;
      }
      if (workletReadyMessage) {
        this.port.onmessage?.({ data: workletReadyMessage } as MessageEvent);
      }
    });
  }
}

export class MockWorker {
  onerror: ((event: ErrorEvent) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onmessageerror: ((event: MessageEvent) => void) | null = null;
  postMessage = workerPostMessage;
  terminate = workerTerminate;

  constructor() {
    workers.push(this);
    queueMicrotask(() => {
      if (workerStartupError) {
        this.onerror?.({
          error: workerStartupError,
          message: workerStartupError.message
        } as ErrorEvent);
        return;
      }
      if (workerStartupMessageError) {
        this.onmessageerror?.({ data: workerStartupMessageError } as MessageEvent);
        return;
      }
      if (workerReadyMessage) {
        this.onmessage?.({ data: workerReadyMessage } as MessageEvent);
      }
    });
  }
}

export const workletNodes: MockAudioWorkletNode[] = [];
export const workers: MockWorker[] = [];

export function setupClientNoiseCancellationTest(loaders: NoiseLoaderMocks): void {
  loaders.loadLyreNoiseWasm.mockReset();
  loaders.loadCachedNoiseModelBundle.mockReset();
  loaders.loadLyreNoiseWasm.mockResolvedValue({
    bytes: new ArrayBuffer(8),
    channels: 1,
    frameSize: 480,
    sampleRateHz: 48_000
  });
  loaders.loadCachedNoiseModelBundle.mockResolvedValue(null);
  workletNodes.length = 0;
  workers.length = 0;
  for (const mock of [
    inputTrackStop,
    outputTrackStop,
    secondOutputTrackStop,
    sourceDisconnect,
    processorDisconnect,
    processorConnect,
    sourceConnect,
    contextClose,
    addModule,
    workerPostMessage,
    workerTerminate
  ]) {
    mock.mockReset();
  }
  contextClose.mockResolvedValue(undefined);
  workerReadyMessage = { type: "ready" };
  workletReadyMessage = { type: "ready" };
  workerStartupError = null;
  workerStartupMessageError = null;
  workletStartupProcessorError = null;
  workletStartupMessageError = null;
  outputTrack.stop = outputTrackStop;
  secondOutputTrack.stop = secondOutputTrackStop;
  destinationAudioTracks = [outputTrack];
  vi.stubGlobal("AudioContext", MockAudioContext);
  vi.stubGlobal("AudioWorkletNode", MockAudioWorkletNode);
  vi.stubGlobal("Worker", MockWorker);
}

export function dpdfnetModelBundle(initialState = [0]): {
  manifest: { initialState: number[] };
  models: Array<{ bytes: ArrayBuffer; name: string; url: string }>;
  provider: "dpdfnet";
} {
  return {
    provider: "dpdfnet",
    manifest: { initialState },
    models: [{
      bytes: new ArrayBuffer(4),
      name: "dpdfnet2_48khz_hr",
      url: "/models/dpdfnet/dpdfnet2_48khz_hr.onnx"
    }]
  };
}

export function setDestinationAudioTracks(tracks: typeof destinationAudioTracks): void {
  destinationAudioTracks = tracks;
}

export function setWorkerReadyMessage(message: StartupMessage | null): void {
  workerReadyMessage = message;
}

export function setWorkletReadyMessage(message: StartupMessage | null): void {
  workletReadyMessage = message;
}

export function setWorkerStartupError(error: Error): void {
  workerStartupError = error;
}

export function setWorkerStartupMessageError(error: Error): void {
  workerStartupMessageError = error;
}

export function setWorkletStartupProcessorError(error: Error): void {
  workletStartupProcessorError = error;
}

export function setWorkletStartupMessageError(error: Error): void {
  workletStartupMessageError = error;
}
