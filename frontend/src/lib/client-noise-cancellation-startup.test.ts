import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { processLocalAudioStream } from "./client-noise-cancellation";
import {
  contextClose,
  dpdfnetModelBundle,
  inputStream,
  inputTrackStop,
  setWorkerReadyMessage,
  setWorkerStartupError,
  setWorkerStartupMessageError,
  setWorkletReadyMessage,
  setWorkletStartupMessageError,
  setWorkletStartupProcessorError,
  setupClientNoiseCancellationTest,
  sourceDisconnect,
  workerPostMessage,
  workerTerminate,
  workletNodes
} from "./client-noise-cancellation.test-support";
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

describe("client noise cancellation startup", () => {
  beforeEach(() => {
    setupClientNoiseCancellationTest(wasmLoader);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("cleans up the context and worker when ONNX initialization fails", async () => {
    wasmLoader.loadCachedNoiseModelBundle.mockResolvedValue(dpdfnetModelBundle());
    setWorkerReadyMessage({ message: "model init failed", type: "error" });

    await expect(processLocalAudioStream(inputStream, {
      noise: defaultNoiseConfig
    })).rejects.toThrow("model init failed");

    expect(workerTerminate).toHaveBeenCalledOnce();
    expect(contextClose).toHaveBeenCalledOnce();
    expect(inputTrackStop).not.toHaveBeenCalled();
  });

  it("times out and releases the audio graph when the ONNX worker never becomes ready", async () => {
    vi.useFakeTimers();
    wasmLoader.loadCachedNoiseModelBundle.mockResolvedValue(dpdfnetModelBundle());
    setWorkerReadyMessage(null);

    const rejection = expect(processLocalAudioStream(inputStream, {
      noise: defaultNoiseConfig
    })).rejects.toThrow("ONNX noise worker did not become ready");
    await vi.advanceTimersByTimeAsync(60_000);
    await rejection;

    expect(workerTerminate).toHaveBeenCalledOnce();
    expect(sourceDisconnect).toHaveBeenCalledOnce();
    expect(contextClose).toHaveBeenCalledOnce();
  });

  it("preserves a worker startup error and releases the unfinished audio graph", async () => {
    const startupError = new Error("worker module failed to load");
    wasmLoader.loadCachedNoiseModelBundle.mockResolvedValue(dpdfnetModelBundle());
    setWorkerStartupError(startupError);

    await expect(processLocalAudioStream(inputStream, {
      noise: defaultNoiseConfig
    })).rejects.toBe(startupError);

    expect(workerTerminate).toHaveBeenCalledOnce();
    expect(sourceDisconnect).toHaveBeenCalledOnce();
    expect(contextClose).toHaveBeenCalledOnce();
    expect(inputTrackStop).not.toHaveBeenCalled();
  });

  it("rejects unreadable worker startup messages and releases the worker", async () => {
    const messageError = new Error("worker startup message could not be cloned");
    wasmLoader.loadCachedNoiseModelBundle.mockResolvedValue(dpdfnetModelBundle());
    setWorkerStartupMessageError(messageError);

    await expect(processLocalAudioStream(inputStream, {
      noise: defaultNoiseConfig
    })).rejects.toBe(messageError);

    expect(workerTerminate).toHaveBeenCalledOnce();
    expect(sourceDisconnect).toHaveBeenCalledOnce();
    expect(contextClose).toHaveBeenCalledOnce();
  });

  it("releases the worker timer when posting its initialization message fails", async () => {
    vi.useFakeTimers();
    const postError = new Error("worker init message could not be posted");
    wasmLoader.loadCachedNoiseModelBundle.mockResolvedValue(dpdfnetModelBundle());
    workerPostMessage.mockImplementationOnce(() => {
      throw postError;
    });

    await expect(processLocalAudioStream(inputStream, {
      noise: defaultNoiseConfig
    })).rejects.toBe(postError);

    expect(workerTerminate).toHaveBeenCalledOnce();
    expect(sourceDisconnect).toHaveBeenCalledOnce();
    expect(contextClose).toHaveBeenCalledOnce();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("disposes the worklet when worklet initialization fails", async () => {
    setWorkletReadyMessage({ message: "worklet init failed", type: "error" });

    await expect(processLocalAudioStream(inputStream, {
      noise: defaultNoiseConfig
    })).rejects.toThrow("worklet init failed");

    expect(workletNodes[0].port.postMessage).toHaveBeenCalledWith({ type: "dispose" });
    expect(workletNodes[0].onprocessorerror).toBeNull();
    expect(sourceDisconnect).toHaveBeenCalledOnce();
    expect(contextClose).toHaveBeenCalledOnce();
    expect(inputTrackStop).not.toHaveBeenCalled();
  });

  it("preserves the startup error when closing the failed audio context also fails", async () => {
    const startupError = new Error("worklet init failed");
    const closeError = new Error("audio context close failed");
    const onError = vi.fn();
    setWorkletReadyMessage({ message: startupError.message, type: "error" });
    contextClose.mockRejectedValueOnce(closeError);

    await expect(processLocalAudioStream(inputStream, {
      noise: defaultNoiseConfig,
      onError
    })).rejects.toThrow(startupError.message);

    expect(contextClose).toHaveBeenCalledOnce();
    expect(onError).toHaveBeenCalledWith(closeError);
  });

  it("times out and disposes the audio graph when the worklet never becomes ready", async () => {
    vi.useFakeTimers();
    setWorkletReadyMessage(null);

    const rejection = expect(processLocalAudioStream(inputStream, {
      noise: defaultNoiseConfig
    })).rejects.toThrow("audio worklet did not become ready");
    await vi.advanceTimersByTimeAsync(60_000);
    await rejection;

    expect(workletNodes[0].port.postMessage).toHaveBeenCalledWith({ type: "dispose" });
    expect(sourceDisconnect).toHaveBeenCalledOnce();
    expect(contextClose).toHaveBeenCalledOnce();
  });

  it("preserves a worklet processor startup error and releases the audio graph", async () => {
    const processorError = new Error("audio worklet processor crashed during startup");
    setWorkletStartupProcessorError(processorError);

    await expect(processLocalAudioStream(inputStream, {
      noise: defaultNoiseConfig
    })).rejects.toBe(processorError);

    expect(workletNodes[0].port.postMessage).toHaveBeenCalledWith({ type: "dispose" });
    expect(sourceDisconnect).toHaveBeenCalledOnce();
    expect(contextClose).toHaveBeenCalledOnce();
  });

  it("preserves an unreadable worklet startup message error and releases the audio graph", async () => {
    const messageError = new Error("audio worklet startup message could not be cloned");
    setWorkletStartupMessageError(messageError);

    await expect(processLocalAudioStream(inputStream, {
      noise: defaultNoiseConfig
    })).rejects.toBe(messageError);

    expect(workletNodes[0].port.postMessage).toHaveBeenCalledWith({ type: "dispose" });
    expect(sourceDisconnect).toHaveBeenCalledOnce();
    expect(contextClose).toHaveBeenCalledOnce();
  });
});
