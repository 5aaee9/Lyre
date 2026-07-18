import { beforeAll, describe, expect, it, vi } from "vitest";

let RegisteredProcessor;
let lastWasm;

class MockMessagePort {
  constructor() {
    this.close = vi.fn();
    this.onmessage = null;
    this.postMessage = vi.fn();
  }
}

class MockAudioWorkletProcessor {
  constructor() {
    this.port = new MockMessagePort();
  }
}

function fakeWasm() {
  const memory = { buffer: new ArrayBuffer(32_768) };
  let nextPointer = 4;
  lastWasm = {
    memory,
    lyre_noise_wasm_alloc_f32(length) {
      const pointer = nextPointer;
      nextPointer += length * Float32Array.BYTES_PER_ELEMENT;
      return pointer;
    },
    lyre_noise_wasm_deepfilternet_coefs_len: () => 2,
    lyre_noise_wasm_deepfilternet_df_bins: () => 1,
    lyre_noise_wasm_deepfilternet_df_order: () => 1,
    lyre_noise_wasm_deepfilternet_erb_bands: () => 2,
    lyre_noise_wasm_dealloc_f32: vi.fn(),
    lyre_noise_wasm_deepfilternet_features: () => 1,
    lyre_noise_wasm_deepfilternet_free: vi.fn(),
    lyre_noise_wasm_deepfilternet_new: () => 1,
    lyre_noise_wasm_deepfilternet_synthesize: vi.fn(function (
      _processor,
      _maskPtr,
      _maskLen,
      _coefsPtr,
      _coefsLen,
      outputPtr,
      frameSize
    ) {
      new Float32Array(memory.buffer).fill(
        0.9,
        outputPtr / Float32Array.BYTES_PER_ELEMENT,
        outputPtr / Float32Array.BYTES_PER_ELEMENT + frameSize
      );
      return 1;
    }),
    lyre_noise_wasm_dpdfnet_free: vi.fn(),
    lyre_noise_wasm_dpdfnet_istft: vi.fn(function (
      _processor,
      _specPtr,
      _specLen,
      outputPtr,
      frameSize
    ) {
      new Float32Array(memory.buffer).fill(
        0.8,
        outputPtr / Float32Array.BYTES_PER_ELEMENT,
        outputPtr / Float32Array.BYTES_PER_ELEMENT + frameSize
      );
      return 1;
    }),
    lyre_noise_wasm_dpdfnet_new: () => 2,
    lyre_noise_wasm_dpdfnet_stft: () => 1
  };
  return lastWasm;
}

beforeAll(async () => {
  vi.stubGlobal("AudioWorkletProcessor", MockAudioWorkletProcessor);
  vi.stubGlobal("registerProcessor", (_name, processor) => {
    RegisteredProcessor = processor;
  });
  vi.stubGlobal("WebAssembly", {
    instantiate: vi.fn(async () => ({
      instance: { exports: fakeWasm() }
    }))
  });
  await import("../../public/audio-worklets/lyre-noise-processor.js");
});

describe("Lyre ONNX audio worklet", () => {
  it("plays inference that completes within two following model frames", async () => {
    const processor = new RegisteredProcessor({
      processorOptions: {
        frameSize: 4,
        hasOnnxModels: true,
        processor: "deepfilternet",
        wasmBytes: new ArrayBuffer(8)
      }
    });
    await vi.waitFor(() => {
      expect(processor.port.postMessage).toHaveBeenCalledWith({ type: "ready" });
    });
    const onnxPort = new MockMessagePort();
    processor.port.onmessage({ data: { port: onnxPort, type: "onnx-port" } });

    processor.process(
      [[new Float32Array([0.1, 0.2, 0.3, 0.4])]],
      [[new Float32Array(4)]]
    );
    for (let index = 0; index < 4; index += 1) {
      processor.process(
        [[new Float32Array([0.5 + index / 10])]],
        [[new Float32Array(1)]]
      );
    }
    onnxPort.onmessage({
      data: { id: 1, output: new Float32Array(4), type: "processed" }
    });
    const output = new Float32Array(4);
    processor.process(
      [[new Float32Array([1.1, 1.2, 1.3, 1.4])]],
      [[output]]
    );

    expect(output).toEqual(new Float32Array([0.9, 0.9, 0.9, 0.9]));
  });

  it("bounds slow inference to one request and falls back without a stale output burst", async () => {
    const processor = new RegisteredProcessor({
      processorOptions: {
        frameSize: 4,
        hasOnnxModels: true,
        processor: "deepfilternet",
        wasmBytes: new ArrayBuffer(8)
      }
    });
    await vi.waitFor(() => {
      expect(processor.port.postMessage).toHaveBeenCalledWith({ type: "ready" });
    });
    const onnxPort = new MockMessagePort();
    processor.port.onmessage({ data: { port: onnxPort, type: "onnx-port" } });

    const firstInput = new Float32Array([0.1, 0.2, 0.3, 0.4]);
    const firstOutput = new Float32Array(4);
    processor.process([[firstInput]], [[firstOutput]]);
    expect(firstOutput).toEqual(new Float32Array(4));

    const rendered = [];
    for (let index = 0; index < 64; index += 1) {
      const output = new Float32Array(4);
      processor.process(
        [[new Float32Array([1 + index, 2 + index, 3 + index, 4 + index])]],
        [[output]]
      );
      rendered.push(...output);
    }

    expect(onnxPort.postMessage).toHaveBeenCalledTimes(1);
    expect(rendered.slice(12).some((sample) => sample !== 0)).toBe(true);
    expect(processor.outputQueue.length).toBeLessThanOrEqual(12);

    onnxPort.onmessage({
      data: { id: 1, output: new Float32Array(4), type: "processed" }
    });
    const afterLateResult = [];
    for (let index = 0; index < 8; index += 1) {
      const output = new Float32Array(4);
      processor.process(
        [[new Float32Array([70 + index * 4, 71 + index * 4, 72 + index * 4, 73 + index * 4])]],
        [[output]]
      );
      afterLateResult.push(...output);
    }

    expect(onnxPort.postMessage).toHaveBeenCalledTimes(1);
    expect(lastWasm.lyre_noise_wasm_deepfilternet_synthesize).not.toHaveBeenCalled();
    expect(afterLateResult).not.toContain(0.9);
    expect(afterLateResult).toContain(70);
    expect(processor.port.postMessage).toHaveBeenCalledWith({
      message: "ONNX inference exceeded the realtime latency budget; raw audio bypass enabled",
      type: "error"
    });
    processor.port.onmessage({ data: { type: "dispose" } });
    processor.port.onmessage({ data: { type: "dispose" } });
    expect(lastWasm.lyre_noise_wasm_deepfilternet_free).toHaveBeenCalledOnce();
    expect(lastWasm.lyre_noise_wasm_dealloc_f32).toHaveBeenCalledTimes(6);
    expect(onnxPort.close).toHaveBeenCalledOnce();
  });

  it("permanently bypasses ONNX after a failed inference", async () => {
    const processor = new RegisteredProcessor({
      processorOptions: {
        frameSize: 4,
        hasOnnxModels: true,
        processor: "deepfilternet",
        wasmBytes: new ArrayBuffer(8)
      }
    });
    await vi.waitFor(() => {
      expect(processor.port.postMessage).toHaveBeenCalledWith({ type: "ready" });
    });
    const onnxPort = new MockMessagePort();
    processor.port.onmessage({ data: { port: onnxPort, type: "onnx-port" } });
    const firstInput = new Float32Array([0.1, 0.2, 0.3, 0.4]);

    processor.process([[firstInput]], [[new Float32Array(4)]]);
    onnxPort.onmessage({
      data: { id: 1, message: "inference failed", type: "error" }
    });
    onnxPort.onmessage({
      data: { id: 1, message: "late duplicate inference failure", type: "error" }
    });
    const output = new Float32Array(4);
    processor.process(
      [[new Float32Array([0.5, 0.6, 0.7, 0.8])]],
      [[output]]
    );
    const followingRaw = [];
    for (let index = 0; index < 6; index += 1) {
      const followingOutput = new Float32Array(4);
      processor.process(
        [[new Float32Array([1 + index * 4, 2 + index * 4, 3 + index * 4, 4 + index * 4])]],
        [[followingOutput]]
      );
      followingRaw.push(...followingOutput);
    }

    expect(output).toEqual(firstInput);
    expect(onnxPort.postMessage).toHaveBeenCalledTimes(1);
    expect(lastWasm.lyre_noise_wasm_deepfilternet_synthesize).not.toHaveBeenCalled();
    expect(followingRaw).toContain(0.5);
    expect(followingRaw).not.toContain(0.9);
    expect(processor.port.postMessage).toHaveBeenCalledWith({
      id: 1,
      message: "inference failed",
      type: "error"
    });
    expect(processor.port.postMessage).toHaveBeenCalledTimes(2);
  });

  it("preserves the pending raw frame and bypasses ONNX when synthesis fails", async () => {
    const processor = new RegisteredProcessor({
      processorOptions: {
        frameSize: 4,
        hasOnnxModels: true,
        processor: "deepfilternet",
        wasmBytes: new ArrayBuffer(8)
      }
    });
    await vi.waitFor(() => {
      expect(processor.port.postMessage).toHaveBeenCalledWith({ type: "ready" });
    });
    const onnxPort = new MockMessagePort();
    processor.port.onmessage({ data: { port: onnxPort, type: "onnx-port" } });
    const firstInput = new Float32Array([0.1, 0.2, 0.3, 0.4]);

    processor.process([[firstInput]], [[new Float32Array(4)]]);
    lastWasm.lyre_noise_wasm_deepfilternet_synthesize.mockReturnValueOnce(0);
    onnxPort.onmessage({
      data: { id: 1, output: new Float32Array(4), type: "processed" }
    });
    const output = new Float32Array(4);
    processor.process(
      [[new Float32Array([0.5, 0.6, 0.7, 0.8])]],
      [[output]]
    );

    expect(output).toEqual(firstInput);
    expect(onnxPort.postMessage).toHaveBeenCalledTimes(1);
    expect(processor.port.postMessage).toHaveBeenCalledWith({
      message: "DeepFilterNet synthesis failed; raw audio bypass enabled",
      type: "error"
    });
  });

  it("preserves the pending raw frame and bypasses ONNX when inverse STFT fails", async () => {
    const processor = new RegisteredProcessor({
      processorOptions: {
        frameSize: 480,
        hasOnnxModels: true,
        processor: "dpdfnet",
        wasmBytes: new ArrayBuffer(8)
      }
    });
    await vi.waitFor(() => {
      expect(processor.port.postMessage).toHaveBeenCalledWith({ type: "ready" });
    });
    const onnxPort = new MockMessagePort();
    processor.port.onmessage({ data: { port: onnxPort, type: "onnx-port" } });
    const firstInput = Float32Array.from({ length: 480 }, (_, index) => index / 480);

    processor.process([[firstInput]], [[new Float32Array(480)]]);
    lastWasm.lyre_noise_wasm_dpdfnet_istft.mockReturnValueOnce(0);
    onnxPort.onmessage({
      data: { id: 1, output: new Float32Array(962), type: "processed" }
    });
    const output = new Float32Array(480);
    processor.process([[new Float32Array(480).fill(0.5)]], [[output]]);

    expect(output).toEqual(firstInput);
    expect(onnxPort.postMessage).toHaveBeenCalledTimes(1);
    expect(processor.port.postMessage).toHaveBeenCalledWith({
      message: "DPDFNet inverse STFT failed; raw audio bypass enabled",
      type: "error"
    });
  });

  it("disposes WASM allocations exactly once", async () => {
    const processor = new RegisteredProcessor({
      processorOptions: {
        frameSize: 4,
        hasOnnxModels: true,
        processor: "deepfilternet",
        wasmBytes: new ArrayBuffer(8)
      }
    });
    await vi.waitFor(() => {
      expect(processor.port.postMessage).toHaveBeenCalledWith({ type: "ready" });
    });

    processor.port.onmessage({ data: { type: "dispose" } });
    processor.port.onmessage({ data: { type: "dispose" } });
    const output = new Float32Array(4);

    expect(processor.process([[new Float32Array(4)]], [[output]])).toBe(false);
    expect(lastWasm.lyre_noise_wasm_deepfilternet_free).toHaveBeenCalledOnce();
    expect(lastWasm.lyre_noise_wasm_dealloc_f32).toHaveBeenCalledTimes(6);
  });
});
