class LyreNoiseProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    this.frameSize = options.processorOptions.frameSize;
    this.inputFrame = new Float32Array(this.frameSize);
    this.outputQueue = [];
    this.inputOffset = 0;
    this.ready = false;
    this.initialise(options.processorOptions.wasmBytes);
  }

  async initialise(wasmBytes) {
    try {
      const { instance } = await WebAssembly.instantiate(wasmBytes, {});
      this.wasm = instance.exports;
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
}

registerProcessor("lyre-noise-processor", LyreNoiseProcessor);
