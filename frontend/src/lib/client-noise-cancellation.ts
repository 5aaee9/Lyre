import { loadLyreNoiseWasm } from "./lyre-noise-wasm";
import type { NoiseCancellationConfig } from "./api";
import { loadCachedNoiseModelBundle } from "./noise-model-cache";

export type ClientNoiseProcessorKind = "rnnoise" | "deepfilternet" | "dpdfnet";

export type ClientNoiseProcessorOptions = {
  noise: NoiseCancellationConfig;
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

export async function processLocalAudioStream(
  input: MediaStream,
  options: ClientNoiseProcessorOptions
): Promise<MediaStream> {
  const AudioContextClass = resolveAudioContext();
  const context = new AudioContextClass({ sampleRate: 48_000 });
  const wasm = await loadLyreNoiseWasm();
  if (context.sampleRate !== wasm.sampleRateHz) {
    await context.close();
    throw new Error(`client noise cancellation requires ${wasm.sampleRateHz} Hz audio context`);
  }

  await context.audioWorklet.addModule("/audio-worklets/lyre-noise-processor.js");
  const source = context.createMediaStreamSource(input);
  const processorKind = clientProcessorKind(options.noise);
  const modelBundle = await loadCachedNoiseModelBundle(options.noise);
  const onnxRuntime = modelBundle
    ? await createOnnxRuntime(modelBundle, options.noise.dpdfnet.model)
    : null;
  const processor = new AudioWorkletNode(context, "lyre-noise-processor", {
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
  const destination = context.createMediaStreamDestination();
  await waitForWorkletReady(processor);
  if (onnxRuntime) {
    processor.port.postMessage({ port: onnxRuntime.port, type: "onnx-port" }, [onnxRuntime.port]);
  }
  source.connect(processor);
  processor.connect(destination);
  const output = destination.stream;
  const rawStop = stopAllAudioTracks(input);
  for (const track of output.getAudioTracks()) {
    const originalStop = track.stop.bind(track);
    track.stop = () => {
      originalStop();
      source.disconnect();
      processor.disconnect();
      onnxRuntime?.worker.terminate();
      rawStop();
      void context.close();
    };
  }
  return output;
}

async function createOnnxRuntime(
  modelBundle: NonNullable<Awaited<ReturnType<typeof loadCachedNoiseModelBundle>>>,
  model: string
): Promise<{ port: MessagePort; worker: Worker }> {
  const worker = new Worker(new URL("./client-noise-onnx-worker.ts", import.meta.url), {
    type: "module"
  });
  const channel = new MessageChannel();
  const transferables = modelBundle.models.map((cachedModel) => cachedModel.bytes);
  const ready = new Promise<void>((resolve, reject) => {
    worker.onmessage = (event: MessageEvent<{ message?: string; type: "error" | "ready" }>) => {
      if (event.data.type === "ready") {
        resolve();
        return;
      }
      reject(new Error(event.data.message ?? "ONNX noise runtime failed to initialize"));
    };
  });
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
  worker.onmessage = null;
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

function waitForWorkletReady(node: AudioWorkletNode): Promise<void> {
  return new Promise((resolve, reject) => {
    node.port.onmessage = (event: MessageEvent<WorkletMessage>) => {
      if (event.data.type === "ready") {
        resolve();
        return;
      }
      reject(new Error(event.data.message));
    };
  });
}

function resolveAudioContext(): AudioContextConstructor {
  return window.AudioContext;
}

function stopAllAudioTracks(stream: MediaStream): () => void {
  return () => {
    for (const track of stream.getAudioTracks()) {
      track.stop();
    }
  };
}
