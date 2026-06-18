type LyreNoiseWasmExports = {
  memory: WebAssembly.Memory;
  lyre_noise_wasm_alloc_f32: (len: number) => number;
  lyre_noise_wasm_dealloc_f32: (ptr: number, len: number) => void;
  lyre_noise_wasm_channels: () => number;
  lyre_noise_wasm_frame_size: () => number;
  lyre_noise_wasm_rnnoise_free: (processor: number) => void;
  lyre_noise_wasm_rnnoise_new: () => number;
  lyre_noise_wasm_rnnoise_process: (
    processor: number,
    inputPtr: number,
    len: number,
    outputPtr: number
  ) => number;
  lyre_noise_wasm_sample_rate_hz: () => number;
};

export type LyreNoiseWasmModule = {
  channels: number;
  exports: LyreNoiseWasmExports;
  frameSize: number;
  bytes: ArrayBuffer;
  sampleRateHz: number;
};

let modulePromise: Promise<LyreNoiseWasmModule> | null = null;

export async function loadLyreNoiseWasm(): Promise<LyreNoiseWasmModule> {
  modulePromise ??= instantiateLyreNoiseWasm();
  return modulePromise;
}

async function instantiateLyreNoiseWasm(): Promise<LyreNoiseWasmModule> {
  const response = await fetch("/wasm/lyre_noise_wasm.wasm");
  const bytes = await response.arrayBuffer();
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const exports = instance.exports as LyreNoiseWasmExports;
  return {
    bytes,
    channels: exports.lyre_noise_wasm_channels(),
    exports,
    frameSize: exports.lyre_noise_wasm_frame_size(),
    sampleRateHz: exports.lyre_noise_wasm_sample_rate_hz()
  };
}
