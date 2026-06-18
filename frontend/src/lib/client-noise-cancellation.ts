import { loadLyreNoiseWasm } from "./lyre-noise-wasm";

type AudioContextConstructor = typeof AudioContext;

type WorkletReadyMessage = {
  type: "ready";
};

type WorkletErrorMessage = {
  message: string;
  type: "error";
};

type WorkletMessage = WorkletReadyMessage | WorkletErrorMessage;

export async function processLocalAudioStream(input: MediaStream): Promise<MediaStream> {
  const AudioContextClass = resolveAudioContext();
  const context = new AudioContextClass({ sampleRate: 48_000 });
  const wasm = await loadLyreNoiseWasm();
  if (context.sampleRate !== wasm.sampleRateHz) {
    await context.close();
    throw new Error(`client noise cancellation requires ${wasm.sampleRateHz} Hz audio context`);
  }

  await context.audioWorklet.addModule("/audio-worklets/lyre-noise-processor.js");
  const source = context.createMediaStreamSource(input);
  const processor = new AudioWorkletNode(context, "lyre-noise-processor", {
    numberOfInputs: 1,
    numberOfOutputs: 1,
    outputChannelCount: [wasm.channels],
    processorOptions: {
      wasmBytes: wasm.bytes,
      frameSize: wasm.frameSize
    }
  });
  const destination = context.createMediaStreamDestination();
  await waitForWorkletReady(processor);
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
      rawStop();
      void context.close();
    };
  }
  return output;
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
