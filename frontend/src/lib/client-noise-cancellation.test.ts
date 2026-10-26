import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { processLocalAudioStream } from "./client-noise-cancellation";
import {
  addModule,
  audioContexts,
  contextClose,
  contextResume,
  destinationStream,
  dpdfnetModelBundle,
  inputStream,
  inputTrackStop,
  outputTrack,
  outputTrackStop,
  processorConnect,
  processorDisconnect,
  secondOutputTrack,
  secondOutputTrackStop,
  setDestinationAudioTracks,
  setupClientNoiseCancellationTest,
  sourceConnect,
  sourceDisconnect,
  workerPostMessage,
  workers,
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

describe("client noise cancellation runtime", () => {
  beforeEach(() => {
    setupClientNoiseCancellationTest(wasmLoader);
  });

  afterEach(() => {
    vi.useRealTimers();
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

  it("resumes the processing context on creation and when it is suspended later", async () => {
    const output = await processLocalAudioStream(inputStream, { noise: defaultNoiseConfig });
    expect(contextResume).toHaveBeenCalledOnce();

    const context = audioContexts[0];
    context.state = "suspended";
    context.onstatechange?.();
    expect(contextResume).toHaveBeenCalledTimes(2);

    context.state = "running";
    context.onstatechange?.();
    expect(contextResume).toHaveBeenCalledTimes(2);

    output.getAudioTracks()[0].stop();
    expect(context.onstatechange).toBeNull();
  });

  it("stops the raw track and closes the graph when the processed track stops", async () => {
    const output = await processLocalAudioStream(inputStream, { noise: defaultNoiseConfig });

    output.getAudioTracks()[0].stop();

    expect(outputTrackStop).toHaveBeenCalledOnce();
    expect(inputTrackStop).toHaveBeenCalledOnce();
    expect(sourceDisconnect).toHaveBeenCalledOnce();
    expect(processorDisconnect).toHaveBeenCalledOnce();
    expect(workletNodes[0].port.postMessage).toHaveBeenCalledWith({ type: "dispose" });
    expect(workletNodes[0].port.onmessage).toBeNull();
    expect(contextClose).toHaveBeenCalledOnce();
  });

  it("stops each output track and releases a shared audio graph exactly once", async () => {
    wasmLoader.loadCachedNoiseModelBundle.mockResolvedValue(dpdfnetModelBundle());
    setDestinationAudioTracks([outputTrack, secondOutputTrack]);
    const output = await processLocalAudioStream(inputStream, { noise: defaultNoiseConfig });

    for (const track of output.getAudioTracks()) {
      track.stop();
      track.stop();
    }

    expect(outputTrackStop).toHaveBeenCalledOnce();
    expect(secondOutputTrackStop).toHaveBeenCalledOnce();
    expect(inputTrackStop).toHaveBeenCalledOnce();
    expect(sourceDisconnect).toHaveBeenCalledOnce();
    expect(processorDisconnect).toHaveBeenCalledOnce();
    expect(workletNodes[0].port.postMessage.mock.calls.filter(
      ([message]) => message.type === "dispose"
    )).toHaveLength(1);
    expect(workerTerminate).toHaveBeenCalledOnce();
    expect(contextClose).toHaveBeenCalledOnce();
  });

  it("continues releasing the full audio graph when individual cleanup steps fail", async () => {
    const outputStopError = new Error("output track stop failed");
    const sourceError = new Error("source disconnect failed");
    const inputStopError = new Error("input track stop failed");
    const closeError = new Error("audio context close failed");
    const onError = vi.fn();
    wasmLoader.loadCachedNoiseModelBundle.mockResolvedValue(dpdfnetModelBundle());
    outputTrackStop.mockImplementationOnce(() => {
      throw outputStopError;
    });
    sourceDisconnect.mockImplementationOnce(() => {
      throw sourceError;
    });
    inputTrackStop.mockImplementationOnce(() => {
      throw inputStopError;
    });
    contextClose.mockRejectedValueOnce(closeError);
    const output = await processLocalAudioStream(inputStream, {
      noise: defaultNoiseConfig,
      onError
    });

    expect(() => output.getAudioTracks()[0].stop()).not.toThrow();
    await vi.waitFor(() => expect(onError).toHaveBeenCalledWith(closeError));

    expect(processorDisconnect).toHaveBeenCalledOnce();
    expect(workerTerminate).toHaveBeenCalledOnce();
    expect(contextClose).toHaveBeenCalledOnce();
    expect(onError).toHaveBeenCalledWith(outputStopError);
    expect(onError).toHaveBeenCalledWith(sourceError);
    expect(onError).toHaveBeenCalledWith(inputStopError);
  });

  it("reports worklet failures that happen after initialization", async () => {
    const onError = vi.fn();
    await processLocalAudioStream(inputStream, { noise: defaultNoiseConfig, onError });

    workletNodes[0].port.onmessage?.({
      data: { message: "inference crashed", type: "error" }
    } as MessageEvent);

    expect(onError.mock.calls[0][0]).toEqual(new Error("inference crashed"));
  });

  it("preserves worklet processor failures that happen after initialization", async () => {
    const processorError = new Error("audio worklet processor crashed");
    const onError = vi.fn();
    await processLocalAudioStream(inputStream, { noise: defaultNoiseConfig, onError });

    workletNodes[0].onprocessorerror?.({
      error: processorError,
      message: processorError.message
    } as ErrorEvent);

    expect(onError).toHaveBeenCalledWith(processorError);
  });

  it("preserves unreadable worklet messages that happen after initialization", async () => {
    const messageError = new Error("audio worklet runtime message could not be cloned");
    const onError = vi.fn();
    await processLocalAudioStream(inputStream, { noise: defaultNoiseConfig, onError });

    workletNodes[0].port.onmessageerror?.({ data: messageError } as MessageEvent);

    expect(onError).toHaveBeenCalledWith(messageError);
  });

  it("reports worker failures that happen after initialization", async () => {
    const runtimeError = new Error("worker terminated unexpectedly");
    const onError = vi.fn();
    wasmLoader.loadCachedNoiseModelBundle.mockResolvedValue(dpdfnetModelBundle());
    await processLocalAudioStream(inputStream, { noise: defaultNoiseConfig, onError });

    workers[0].onerror?.({ error: runtimeError, message: runtimeError.message } as ErrorEvent);

    expect(onError).toHaveBeenCalledWith(runtimeError);
    destinationStream.getAudioTracks()[0].stop();
    expect(workers[0].onerror).toBeNull();
    expect(workers[0].onmessage).toBeNull();
    expect(workers[0].onmessageerror).toBeNull();
  });

  it("uses RNNoise in the worklet for non-ONNX client providers", async () => {
    await expect(processLocalAudioStream(inputStream, {
      noise: { ...defaultNoiseConfig, provider: "rnnoise" }
    })).resolves.toBe(destinationStream);

    expect(workletNodes[0].options.processorOptions).toMatchObject({ processor: "rnnoise" });
  });

  it("passes cached ONNX models into the worklet", async () => {
    wasmLoader.loadCachedNoiseModelBundle.mockResolvedValue({
      provider: "deepfilternet",
      models: [{ bytes: new ArrayBuffer(4), name: "enc", url: "/models/deepfilternet/enc.onnx" }]
    });

    await expect(processLocalAudioStream(inputStream, {
      noise: { ...defaultNoiseConfig, provider: "deepfilternet" }
    })).resolves.toBe(destinationStream);

    expect(workletNodes[0].options.processorOptions).toMatchObject({
      hasOnnxModels: true,
      processor: "deepfilternet"
    });
    expect(workerPostMessage).toHaveBeenCalledWith(
      expect.objectContaining({ provider: "deepfilternet", type: "init" }),
      expect.any(Array)
    );
    expect(workletNodes[0].port.postMessage).toHaveBeenCalledWith(
      expect.objectContaining({ type: "onnx-port" }),
      expect.any(Array)
    );
  });

  it("passes the cached DPDFNet manifest into the ONNX worker", async () => {
    const modelBundle = dpdfnetModelBundle([0.25, 0.5]);
    modelBundle.models[0].name = "dpdfnet8_48khz_hr";
    wasmLoader.loadCachedNoiseModelBundle.mockResolvedValue(modelBundle);

    await expect(processLocalAudioStream(inputStream, {
      noise: {
        ...defaultNoiseConfig,
        provider: "dpdfnet",
        dpdfnet: { model: "dpdfnet8_48khz_hr" }
      }
    })).resolves.toBe(destinationStream);

    expect(workerPostMessage).toHaveBeenCalledWith(
      expect.objectContaining({
        manifest: { initialState: [0.25, 0.5] },
        model: "dpdfnet8_48khz_hr",
        provider: "dpdfnet",
        type: "init"
      }),
      expect.any(Array)
    );
  });
});
