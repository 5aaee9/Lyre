class LyreNoiseProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    this.processorKind = options.processorOptions.processor;
    this.hasOnnxModels = options.processorOptions.hasOnnxModels;
    this.frameSize = options.processorOptions.frameSize;
    this.inputFrame = new Float32Array(this.frameSize);
    this.outputQueue = [];
    this.pendingOnnxFrames = new Map();
    this.nextFrameId = 1;
    this.inputOffset = 0;
    this.ready = false;
    this.port.onmessage = (event) => {
      if (event.data.type === "onnx-port") {
        this.attachOnnxPort(event.data.port);
      }
    };
    this.initialise(options.processorOptions.wasmBytes);
  }

  async initialise(wasmBytes) {
    try {
      const { instance } = await WebAssembly.instantiate(wasmBytes, {});
      this.wasm = instance.exports;
      if (this.processorKind !== "rnnoise") {
        this.initialiseOnnxProcessor();
        return;
      }
      this.processor = this.wasm.lyre_noise_wasm_rnnoise_new();
      this.inputPtr = this.wasm.lyre_noise_wasm_alloc_f32(this.frameSize);
      this.outputPtr = this.wasm.lyre_noise_wasm_alloc_f32(this.frameSize);
      this.ready = true;
      this.port.postMessage({ type: "ready" });
    } catch (error) {
      this.port.postMessage({
        type: "error",
        message: error instanceof Error ? error.message : "Failed to initialize Lyre noise processor"
      });
    }
  }

  initialiseOnnxProcessor() {
    if (!this.hasOnnxModels) {
      throw new Error(`${this.processorKind} client noise cancellation model is unavailable`);
    }
    if (this.processorKind === "dpdfnet") {
      this.dpdfnetWindowLen = 960;
      this.dpdfnetHopSize = 480;
      this.dpdfnetSpecLen = (this.dpdfnetWindowLen / 2 + 1) * 2;
      this.processor = this.wasm.lyre_noise_wasm_dpdfnet_new(this.dpdfnetWindowLen, this.dpdfnetHopSize);
      this.inputPtr = this.wasm.lyre_noise_wasm_alloc_f32(this.dpdfnetHopSize);
      this.specPtr = this.wasm.lyre_noise_wasm_alloc_f32(this.dpdfnetSpecLen);
      this.outputPtr = this.wasm.lyre_noise_wasm_alloc_f32(this.dpdfnetHopSize);
      this.frameSize = this.dpdfnetHopSize;
      this.inputFrame = new Float32Array(this.frameSize);
    } else {
      this.deepfilternetFeatErbLen = this.wasm.lyre_noise_wasm_deepfilternet_erb_bands();
      this.deepfilternetFeatSpecLen = this.wasm.lyre_noise_wasm_deepfilternet_df_bins() * 2;
      this.deepfilternetCoefsLen =
        this.wasm.lyre_noise_wasm_deepfilternet_df_bins() *
        this.wasm.lyre_noise_wasm_deepfilternet_df_order() *
        2;
      this.processor = this.wasm.lyre_noise_wasm_deepfilternet_new();
      this.inputPtr = this.wasm.lyre_noise_wasm_alloc_f32(this.frameSize);
      this.featErbPtr = this.wasm.lyre_noise_wasm_alloc_f32(this.deepfilternetFeatErbLen);
      this.featSpecPtr = this.wasm.lyre_noise_wasm_alloc_f32(this.deepfilternetFeatSpecLen);
      this.maskPtr = this.wasm.lyre_noise_wasm_alloc_f32(this.deepfilternetFeatErbLen);
      this.coefsPtr = this.wasm.lyre_noise_wasm_alloc_f32(this.deepfilternetCoefsLen);
      this.outputPtr = this.wasm.lyre_noise_wasm_alloc_f32(this.frameSize);
    }
    this.ready = true;
    this.port.postMessage({ type: "ready" });
  }

  attachOnnxPort(port) {
    this.onnxPort = port;
    this.onnxPort.onmessage = (event) => {
      if (event.data.type === "processed") {
        this.finishOnnxFrame(event.data.id, event.data.output);
      }
      if (event.data.type === "error") {
        this.port.postMessage(event.data);
      }
    };
  }

  process(inputs, outputs) {
    const input = inputs[0]?.[0];
    const output = outputs[0]?.[0];
    if (!output) {
      return true;
    }
    if (!input || !this.ready) {
      output.fill(0);
      return true;
    }
    for (let index = 0; index < output.length; index += 1) {
      this.inputFrame[this.inputOffset] = input[index] ?? 0;
      this.inputOffset += 1;
      if (this.inputOffset === this.frameSize) {
        this.processFrame();
        this.inputOffset = 0;
      }
      output[index] = this.outputQueue.shift() ?? 0;
    }
    return true;
  }

  processFrame() {
    if (this.processorKind === "dpdfnet") {
      this.processDpdfNetFrame();
      return;
    }
    if (this.processorKind === "deepfilternet") {
      this.processDeepFilterNetFrame();
      return;
    }
    const memory = new Float32Array(this.wasm.memory.buffer);
    memory.set(this.inputFrame, this.inputPtr / Float32Array.BYTES_PER_ELEMENT);
    this.wasm.lyre_noise_wasm_rnnoise_process(
      this.processor,
      this.inputPtr,
      this.frameSize,
      this.outputPtr
    );
    const output = memory.slice(
      this.outputPtr / Float32Array.BYTES_PER_ELEMENT,
      this.outputPtr / Float32Array.BYTES_PER_ELEMENT + this.frameSize
    );
    this.outputQueue.push(...output);
  }

  processDpdfNetFrame() {
    if (!this.onnxPort) {
      this.outputQueue.push(...new Float32Array(this.frameSize));
      return;
    }
    const memory = new Float32Array(this.wasm.memory.buffer);
    memory.set(this.inputFrame, this.inputPtr / Float32Array.BYTES_PER_ELEMENT);
    const ok = this.wasm.lyre_noise_wasm_dpdfnet_stft(
      this.processor,
      this.inputPtr,
      this.frameSize,
      this.specPtr,
      this.dpdfnetSpecLen
    );
    if (!ok) {
      this.outputQueue.push(...this.inputFrame);
      return;
    }
    const spec = memory.slice(
      this.specPtr / Float32Array.BYTES_PER_ELEMENT,
      this.specPtr / Float32Array.BYTES_PER_ELEMENT + this.dpdfnetSpecLen
    );
    this.sendOnnxFrame({ spec });
  }

  processDeepFilterNetFrame() {
    if (!this.onnxPort) {
      this.outputQueue.push(...new Float32Array(this.frameSize));
      return;
    }
    const memory = new Float32Array(this.wasm.memory.buffer);
    memory.set(this.inputFrame, this.inputPtr / Float32Array.BYTES_PER_ELEMENT);
    const ok = this.wasm.lyre_noise_wasm_deepfilternet_features(
      this.processor,
      this.inputPtr,
      this.frameSize,
      this.featErbPtr,
      this.deepfilternetFeatErbLen,
      this.featSpecPtr,
      this.deepfilternetFeatSpecLen
    );
    if (!ok) {
      this.outputQueue.push(...this.inputFrame);
      return;
    }
    const featErb = memory.slice(
      this.featErbPtr / Float32Array.BYTES_PER_ELEMENT,
      this.featErbPtr / Float32Array.BYTES_PER_ELEMENT + this.deepfilternetFeatErbLen
    );
    const featSpec = memory.slice(
      this.featSpecPtr / Float32Array.BYTES_PER_ELEMENT,
      this.featSpecPtr / Float32Array.BYTES_PER_ELEMENT + this.deepfilternetFeatSpecLen
    );
    this.sendOnnxFrame({ featErb, featSpec });
  }

  sendOnnxFrame(payload) {
    const id = this.nextFrameId;
    this.nextFrameId += 1;
    this.pendingOnnxFrames.set(id, this.inputFrame.slice());
    const transfer = [];
    for (const value of Object.values(payload)) {
      transfer.push(value.buffer);
    }
    this.onnxPort.postMessage({ id, type: "process", ...payload }, transfer);
  }

  finishOnnxFrame(id, output) {
    if (!this.pendingOnnxFrames.has(id)) {
      return;
    }
    this.pendingOnnxFrames.delete(id);
    if (this.processorKind === "dpdfnet") {
      this.finishDpdfNetFrame(output);
      return;
    }
    this.finishDeepFilterNetFrame(output);
  }

  finishDpdfNetFrame(spec) {
    const memory = new Float32Array(this.wasm.memory.buffer);
    memory.set(spec, this.specPtr / Float32Array.BYTES_PER_ELEMENT);
    const ok = this.wasm.lyre_noise_wasm_dpdfnet_istft(
      this.processor,
      this.specPtr,
      this.dpdfnetSpecLen,
      this.outputPtr,
      this.frameSize
    );
    if (!ok) {
      return;
    }
    const output = memory.slice(
      this.outputPtr / Float32Array.BYTES_PER_ELEMENT,
      this.outputPtr / Float32Array.BYTES_PER_ELEMENT + this.frameSize
    );
    this.outputQueue.push(...output);
  }

  finishDeepFilterNetFrame(modelOutput) {
    const memory = new Float32Array(this.wasm.memory.buffer);
    const mask = modelOutput.slice(0, this.deepfilternetFeatErbLen);
    const coefs = modelOutput.slice(this.deepfilternetFeatErbLen);
    memory.set(mask, this.maskPtr / Float32Array.BYTES_PER_ELEMENT);
    memory.set(coefs, this.coefsPtr / Float32Array.BYTES_PER_ELEMENT);
    const ok = this.wasm.lyre_noise_wasm_deepfilternet_synthesize(
      this.processor,
      this.maskPtr,
      this.deepfilternetFeatErbLen,
      this.coefsPtr,
      this.deepfilternetCoefsLen,
      this.outputPtr,
      this.frameSize
    );
    if (!ok) {
      return;
    }
    const output = memory.slice(
      this.outputPtr / Float32Array.BYTES_PER_ELEMENT,
      this.outputPtr / Float32Array.BYTES_PER_ELEMENT + this.frameSize
    );
    this.outputQueue.push(...output);
  }
}

registerProcessor("lyre-noise-processor", LyreNoiseProcessor);
