import * as ort from "onnxruntime-web/wasm";

type CachedModelMessage = {
  name: string;
  bytes: ArrayBuffer;
};

type InitMessage = {
  type: "init";
  provider: "deepfilternet" | "dpdfnet";
  manifest?: DpdfNetManifestMessage;
  models: CachedModelMessage[];
  model: string;
  port: MessagePort;
};

type DpdfNetManifestMessage = {
  initialState: number[];
};

type ProcessMessage = DpdfNetProcessMessage | DeepFilterNetProcessMessage;

type DpdfNetProcessMessage = {
  id: number;
  type: "process";
  spec: Float32Array;
};

type DeepFilterNetProcessMessage = {
  featErb: Float32Array;
  featSpec: Float32Array;
  id: number;
  type: "process";
};

type DpdfNetRuntime = {
  kind: "dpdfnet";
  session: ort.InferenceSession;
  state: Float32Array;
  inputSpecName: string;
  inputStateName: string;
  outputSpecName: string;
  outputStateName: string;
  windowLen: number;
};

type DeepFilterNetRuntime = {
  kind: "deepfilternet";
  encoder: ort.InferenceSession;
  erbDecoder: ort.InferenceSession;
  dfDecoder: ort.InferenceSession;
};

let runtime: DpdfNetRuntime | DeepFilterNetRuntime | null = null;
let queue = Promise.resolve();

self.onmessage = async (event: MessageEvent<InitMessage>) => {
  try {
    runtime = await createRuntime(event.data);
    event.data.port.onmessage = (message: MessageEvent<ProcessMessage>) => {
      queue = queue.then(() => processPortMessage(event.data.port, message.data));
    };
    self.postMessage({ type: "ready" });
  } catch (error) {
    self.postMessage({
      type: "error",
      message: error instanceof Error ? error.message : "ONNX noise runtime failed"
    });
  }
};

async function processPortMessage(port: MessagePort, message: ProcessMessage): Promise<void> {
  try {
    if (!runtime) {
      throw new Error("ONNX noise runtime is not initialized");
    }
    const output = await processFrame(runtime, message);
    port.postMessage(
      {
        id: message.id,
        type: "processed",
        output
      },
      [output.buffer]
    );
  } catch (error) {
    port.postMessage({
      type: "error",
      message: error instanceof Error ? error.message : "ONNX noise runtime failed"
    });
  }
}

async function createRuntime(message: InitMessage): Promise<DpdfNetRuntime | DeepFilterNetRuntime> {
  ort.env.wasm.numThreads = 1;
  ort.env.wasm.wasmPaths = {
    wasm: "/ort/ort-wasm-simd-threaded.wasm"
  };

  if (message.provider === "dpdfnet") {
    const model = requireModel(message.models, message.model);
    const session = await createSession(model.bytes);
    return {
      kind: "dpdfnet",
      session,
      state: dpdfNetInitialState(session, message),
      inputSpecName: session.inputNames[0],
      inputStateName: session.inputNames[1],
      outputSpecName: session.outputNames[0],
      outputStateName: session.outputNames[1],
      windowLen: dpdfNetWindowLen(message.model)
    };
  }

  return {
    kind: "deepfilternet",
    encoder: await createSession(requireModel(message.models, "enc").bytes),
    erbDecoder: await createSession(requireModel(message.models, "erb_dec").bytes),
    dfDecoder: await createSession(requireModel(message.models, "df_dec").bytes)
  };
}

async function createSession(bytes: ArrayBuffer): Promise<ort.InferenceSession> {
  return ort.InferenceSession.create(bytes, {
    executionProviders: ["wasm"],
    graphOptimizationLevel: "all"
  });
}

async function processFrame(
  currentRuntime: DpdfNetRuntime | DeepFilterNetRuntime,
  message: ProcessMessage
): Promise<Float32Array> {
  if (currentRuntime.kind === "dpdfnet") {
    if (!("spec" in message)) {
      throw new Error("DPDFNet spectrum input is missing");
    }
    const spec = new ort.Tensor("float32", message.spec, [1, 1, currentRuntime.windowLen / 2 + 1, 2]);
    const state = new ort.Tensor("float32", currentRuntime.state, [currentRuntime.state.length]);
    const outputs = await currentRuntime.session.run({
      [currentRuntime.inputSpecName]: spec,
      [currentRuntime.inputStateName]: state
    });
    const outputSpec = outputs[currentRuntime.outputSpecName].data as Float32Array;
    currentRuntime.state = outputs[currentRuntime.outputStateName].data as Float32Array;
    return new Float32Array(outputSpec);
  }

  if (!("featErb" in message)) {
    throw new Error("DeepFilterNet input features are missing");
  }
  const featErb = new ort.Tensor("float32", message.featErb, [1, 1, 1, 32]);
  const featSpec = new ort.Tensor("float32", message.featSpec, [1, 2, 1, 96]);
  const encoded = await currentRuntime.encoder.run({
    [currentRuntime.encoder.inputNames[0]]: featErb,
    [currentRuntime.encoder.inputNames[1]]: featSpec
  });
  const e0 = encoded[currentRuntime.encoder.outputNames[0]].data as Float32Array;
  const e1 = encoded[currentRuntime.encoder.outputNames[1]].data as Float32Array;
  const e2 = encoded[currentRuntime.encoder.outputNames[2]].data as Float32Array;
  const e3 = encoded[currentRuntime.encoder.outputNames[3]].data as Float32Array;
  const emb = encoded[currentRuntime.encoder.outputNames[4]].data as Float32Array;
  const c0 = encoded[currentRuntime.encoder.outputNames[5]].data as Float32Array;
  const erb = await currentRuntime.erbDecoder.run({
    [currentRuntime.erbDecoder.inputNames[0]]: new ort.Tensor("float32", emb, [1, 1, 512]),
    [currentRuntime.erbDecoder.inputNames[1]]: new ort.Tensor("float32", e3, [1, 64, 1, 8]),
    [currentRuntime.erbDecoder.inputNames[2]]: new ort.Tensor("float32", e2, [1, 64, 1, 8]),
    [currentRuntime.erbDecoder.inputNames[3]]: new ort.Tensor("float32", e1, [1, 64, 1, 16]),
    [currentRuntime.erbDecoder.inputNames[4]]: new ort.Tensor("float32", e0, [1, 64, 1, 32])
  });
  const df = await currentRuntime.dfDecoder.run({
    [currentRuntime.dfDecoder.inputNames[0]]: new ort.Tensor("float32", emb, [1, 1, 512]),
    [currentRuntime.dfDecoder.inputNames[1]]: new ort.Tensor("float32", c0, [1, 64, 1, 96])
  });
  return concatFloat32(
    erb[currentRuntime.erbDecoder.outputNames[0]].data as Float32Array,
    df[currentRuntime.dfDecoder.outputNames[0]].data as Float32Array
  );
}

function requireModel(models: CachedModelMessage[], name: string): CachedModelMessage {
  const model = models.find((candidate) => candidate.name === name);
  if (!model) {
    throw new Error(`missing ONNX model ${name}`);
  }
  return model;
}

function dpdfNetInitialState(session: ort.InferenceSession, message: InitMessage): Float32Array {
  if (!message.manifest) {
    throw new Error(`missing DPDFNet model manifest ${message.model}`);
  }
  const state = new Float32Array(message.manifest.initialState);
  const expected = dpdfNetStateLen(session);
  if (expected !== undefined && expected !== state.length) {
    throw new Error(`DPDFNet initial state shape mismatch: expected ${expected}, got ${state.length}`);
  }
  return state;
}

function dpdfNetStateLen(session: ort.InferenceSession): number | undefined {
  const metadata = session.inputMetadata[1];
  const shape = metadata && "shape" in metadata ? metadata.shape : undefined;
  const len = shape?.length === 1 && typeof shape[0] === "number" ? shape[0] : undefined;
  return len;
}

function dpdfNetWindowLen(model: string): number {
  return model.includes("48khz") ? 960 : 320;
}

function concatFloat32(first: Float32Array, second: Float32Array): Float32Array {
  const output = new Float32Array(first.length + second.length);
  output.set(first);
  output.set(second, first.length);
  return output;
}
