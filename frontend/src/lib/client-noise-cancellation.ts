import { loadLyreNoiseWasm } from "./lyre-noise-wasm";
import type { NoiseCancellationConfig } from "./api";
import { loadCachedNoiseModelBundle } from "./noise-model-cache";

export type ClientNoiseProcessorKind = "rnnoise" | "deepfilternet" | "dpdfnet";

export type ClientNoiseProcessorOptions = {
  noise: NoiseCancellationConfig;
  onError?: (error: Error) => void;
};

type AudioContextConstructor = typeof AudioContext;

type WorkletReadyMessage = {
  type: "ready";
};

type WorkletErrorMessage = {
  message: string;
  type: "error";
};

type WorkletMessage = WorkletReadyMessage | WorkletErrorMessage;

type OnnxRuntime = { port: MessagePort; worker: Worker };

const NOISE_PROCESSOR_STARTUP_TIMEOUT_MS = 10_000;

export async function processLocalAudioStream(
  input: MediaStream,
  options: ClientNoiseProcessorOptions
): Promise<MediaStream> {
  const AudioContextClass = resolveAudioContext();
  const context = new AudioContextClass({ sampleRate: 48_000 });
  let source: MediaStreamAudioSourceNode | undefined;
  let processor: AudioWorkletNode | undefined;
  let onnxRuntime: OnnxRuntime | null = null;
  const onRuntimeError = options.onError ?? reportRuntimeError;
  try {
    const wasm = await loadLyreNoiseWasm();
    if (context.sampleRate !== wasm.sampleRateHz) {
      throw new Error(`client noise cancellation requires ${wasm.sampleRateHz} Hz audio context`);
    }

    await context.audioWorklet.addModule("/audio-worklets/lyre-noise-processor.js");
    const activeSource = context.createMediaStreamSource(input);
    source = activeSource;
    const processorKind = clientProcessorKind(options.noise);
    const modelBundle = await loadCachedNoiseModelBundle(options.noise);
    onnxRuntime = modelBundle
      ? await createOnnxRuntime(modelBundle, options.noise.dpdfnet.model, onRuntimeError)
      : null;
    const activeProcessor = new AudioWorkletNode(context, "lyre-noise-processor", {
      numberOfInputs: 1,
      numberOfOutputs: 1,
      outputChannelCount: [wasm.channels],
      processorOptions: {
        processor: processorKind,
        wasmBytes: wasm.bytes,
        frameSize: wasm.frameSize,
        hasOnnxModels: modelBundle !== null,
        model: options.noise.dpdfnet.model
      }
    });
    processor = activeProcessor;
    const destination = context.createMediaStreamDestination();
    await waitForWorkletReady(activeProcessor, onRuntimeError);
    if (onnxRuntime) {
      activeProcessor.port.postMessage(
        { port: onnxRuntime.port, type: "onnx-port" },
        [onnxRuntime.port]
      );
    }
    activeSource.connect(activeProcessor);
    activeProcessor.connect(destination);
    const output = destination.stream;
    const rawTracks = input.getAudioTracks();
    let graphStopped = false;
    const stopAudioGraph = () => {
      if (graphStopped) {
        return;
      }
      graphStopped = true;
      runCleanupSteps([
        () => { activeProcessor.onprocessorerror = null; },
        () => { activeProcessor.port.onmessage = null; },
        () => { activeProcessor.port.onmessageerror = null; },
        () => { activeProcessor.port.postMessage({ type: "dispose" }); },
        () => { activeSource.disconnect(); },
        () => { activeProcessor.disconnect(); },
        () => { if (onnxRuntime) onnxRuntime.worker.onerror = null; },
        () => { if (onnxRuntime) onnxRuntime.worker.onmessage = null; },
        () => { if (onnxRuntime) onnxRuntime.worker.onmessageerror = null; },
        () => { onnxRuntime?.worker.terminate(); },
        ...rawTracks.map((track) => () => { track.stop(); }),
        () => {
          void Promise.resolve(context.close()).catch((closeError: unknown) => {
            notifyCleanupError(onRuntimeError, closeError);
          });
        }
      ], onRuntimeError);
    };
    for (const track of output.getAudioTracks()) {
      const originalStop = track.stop.bind(track);
      let trackStopped = false;
      track.stop = () => {
        if (trackStopped) {
          return;
        }
        trackStopped = true;
        runCleanupSteps([originalStop, stopAudioGraph], onRuntimeError);
      };
    }
    return output;
  } catch (startupError) {
    runCleanupSteps([
      () => { if (processor) processor.onprocessorerror = null; },
      () => { if (processor) processor.port.onmessage = null; },
      () => { if (processor) processor.port.onmessageerror = null; },
      () => { processor?.port.postMessage({ type: "dispose" }); },
      () => { onnxRuntime?.port.close(); },
      () => { if (onnxRuntime) onnxRuntime.worker.onerror = null; },
      () => { if (onnxRuntime) onnxRuntime.worker.onmessage = null; },
      () => { if (onnxRuntime) onnxRuntime.worker.onmessageerror = null; },
      () => { onnxRuntime?.worker.terminate(); },
      () => { source?.disconnect(); }
    ], onRuntimeError);
    try {
      await context.close();
    } catch (closeError) {
      notifyCleanupError(onRuntimeError, closeError);
    }
    throw startupError;
  }
}

async function createOnnxRuntime(
  modelBundle: NonNullable<Awaited<ReturnType<typeof loadCachedNoiseModelBundle>>>,
  model: string,
  onRuntimeError: (error: Error) => void
): Promise<OnnxRuntime> {
  const worker = new Worker(new URL("./client-noise-onnx-worker.ts", import.meta.url), {
    type: "module"
  });
  const channel = new MessageChannel();
  const transferables = modelBundle.models.map((cachedModel) => cachedModel.bytes);
  let startupTimer: number | undefined;
  const clearStartupTimer = () => {
    if (startupTimer !== undefined) {
      window.clearTimeout(startupTimer);
      startupTimer = undefined;
    }
  };
  const ready = new Promise<void>((resolve, reject) => {
    startupTimer = window.setTimeout(() => {
      reject(new Error("ONNX noise worker did not become ready within 10 seconds"));
    }, NOISE_PROCESSOR_STARTUP_TIMEOUT_MS);
    worker.onmessage = (event: MessageEvent<{ message?: string; type: "error" | "ready" }>) => {
      if (event.data.type === "ready") {
        clearStartupTimer();
        resolve();
        return;
      }
      clearStartupTimer();
      reject(new Error(event.data.message ?? "ONNX noise runtime failed to initialize"));
    };
    worker.onerror = (event: ErrorEvent) => {
      clearStartupTimer();
      reject(event.error instanceof Error
        ? event.error
        : new Error(event.message || "ONNX noise worker failed to start"));
    };
    worker.onmessageerror = (event: MessageEvent) => {
      clearStartupTimer();
      reject(event.data instanceof Error
        ? event.data
        : new Error("ONNX noise worker sent an unreadable startup message"));
    };
  });
  try {
    worker.postMessage(
      {
        manifest: modelBundle.provider === "dpdfnet" ? modelBundle.manifest : undefined,
        model,
        models: modelBundle.models,
        port: channel.port1,
        provider: modelBundle.provider,
        type: "init"
      },
      [channel.port1, ...transferables]
    );
    await ready;
  } catch (error) {
    clearStartupTimer();
    worker.onerror = null;
    worker.onmessage = null;
    worker.onmessageerror = null;
    worker.terminate();
    channel.port1.close();
    channel.port2.close();
    throw error;
  }
  worker.onerror = (event: ErrorEvent) => {
    onRuntimeError(event.error instanceof Error
      ? event.error
      : new Error(event.message || "ONNX noise worker failed"));
  };
  worker.onmessage = (event: MessageEvent<{ message?: string; type: "error" | "ready" }>) => {
    if (event.data.type === "error") {
      onRuntimeError(new Error(event.data.message ?? "ONNX noise runtime failed"));
    }
  };
  worker.onmessageerror = (event: MessageEvent) => {
    onRuntimeError(event.data instanceof Error
      ? event.data
      : new Error("ONNX noise worker sent an unreadable runtime message"));
  };
  return {
    port: channel.port2,
    worker
  };
}

function clientProcessorKind(noise: NoiseCancellationConfig): ClientNoiseProcessorKind {
  if (noise.provider === "deepfilternet" || noise.provider === "dpdfnet") {
    return noise.provider;
  }
  return "rnnoise";
}

function waitForWorkletReady(
  node: AudioWorkletNode,
  onRuntimeError: (error: Error) => void
): Promise<void> {
  return new Promise((resolve, reject) => {
    const startupTimer = window.setTimeout(() => {
      reject(new Error("client noise cancellation audio worklet did not become ready within 10 seconds"));
    }, NOISE_PROCESSOR_STARTUP_TIMEOUT_MS);
    node.onprocessorerror = (event: Event) => {
      window.clearTimeout(startupTimer);
      const processorError = (event as ErrorEvent).error;
      reject(processorError instanceof Error
        ? processorError
        : new Error("client noise cancellation audio worklet processor failed to start"));
    };
    node.port.onmessageerror = (event: MessageEvent) => {
      window.clearTimeout(startupTimer);
      reject(event.data instanceof Error
        ? event.data
        : new Error("client noise cancellation audio worklet sent an unreadable startup message"));
    };
    node.port.onmessage = (event: MessageEvent<WorkletMessage>) => {
      if (event.data.type === "ready") {
        window.clearTimeout(startupTimer);
        node.onprocessorerror = (runtimeEvent: Event) => {
          const processorError = (runtimeEvent as ErrorEvent).error;
          onRuntimeError(processorError instanceof Error
            ? processorError
            : new Error("client noise cancellation audio worklet processor failed"));
        };
        node.port.onmessageerror = (runtimeEvent: MessageEvent) => {
          onRuntimeError(runtimeEvent.data instanceof Error
            ? runtimeEvent.data
            : new Error("client noise cancellation audio worklet sent an unreadable runtime message"));
        };
        node.port.onmessage = (runtimeEvent: MessageEvent<WorkletMessage>) => {
          if (runtimeEvent.data.type === "error") {
            onRuntimeError(new Error(runtimeEvent.data.message));
          }
        };
        resolve();
        return;
      }
      window.clearTimeout(startupTimer);
      reject(new Error(event.data.message));
    };
  });
}

function reportRuntimeError(error: Error): void {
  console.error("Client noise cancellation failed after initialization", error);
}

function runCleanupSteps(steps: Array<() => void>, onError: (error: Error) => void): void {
  for (const step of steps) {
    try {
      step();
    } catch (error) {
      notifyCleanupError(onError, error);
    }
  }
}

function notifyCleanupError(onError: (error: Error) => void, error: unknown): void {
  try {
    onError(error instanceof Error ? error : new Error(String(error)));
  } catch (reportError) {
    console.error("Client noise cancellation cleanup error reporting failed", reportError);
  }
}

function resolveAudioContext(): AudioContextConstructor {
  return window.AudioContext;
}
