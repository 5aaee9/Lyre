import { beforeAll, describe, expect, it, vi } from "vitest";

const onnx = vi.hoisted(() => ({
  run: vi.fn(),
  create: vi.fn()
}));

vi.mock("onnxruntime-web/wasm", () => ({
  env: { wasm: {} },
  InferenceSession: { create: onnx.create },
  Tensor: class {
    constructor(
      readonly type: string,
      readonly data: Float32Array,
      readonly dimensions: number[]
    ) {}
  }
}));

const workerScope = {
  onmessage: null as ((event: MessageEvent) => Promise<void>) | null,
  postMessage: vi.fn()
};

beforeAll(async () => {
  onnx.run.mockRejectedValue(new Error("inference failed"));
  onnx.create.mockResolvedValue({
    inputMetadata: [{}, { shape: [1] }],
    inputNames: ["spec", "state"],
    outputNames: ["output", "next-state"],
    run: onnx.run
  });
  vi.stubGlobal("self", workerScope);
  await import("./client-noise-onnx-worker");
});

describe("client noise ONNX worker", () => {
  it("includes the frame id when inference fails", async () => {
    const port = {
      onmessage: null as ((event: MessageEvent) => void) | null,
      postMessage: vi.fn()
    };
    await workerScope.onmessage?.({
      data: {
        manifest: { initialState: [0] },
        model: "dpdfnet8_48khz_hr",
        models: [{ bytes: new ArrayBuffer(4), name: "dpdfnet8_48khz_hr" }],
        port,
        provider: "dpdfnet",
        type: "init"
      }
    } as unknown as MessageEvent);

    port.onmessage?.({
      data: {
        id: 7,
        spec: new Float32Array(962),
        type: "process"
      }
    } as MessageEvent);

    await vi.waitFor(() => {
      expect(port.postMessage).toHaveBeenCalledWith({
        id: 7,
        message: "inference failed",
        type: "error"
      });
    });
  });
});
