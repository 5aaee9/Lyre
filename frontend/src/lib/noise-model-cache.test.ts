import { beforeEach, describe, expect, it, vi } from "vitest";
import { loadCachedNoiseModelBundle } from "./noise-model-cache";
import { defaultNoiseConfig } from "./settings-store";

class CacheResponse {
  constructor(
    readonly body: ArrayBuffer,
    readonly ok = true,
    readonly status = 200
  ) {}

  arrayBuffer() {
    return Promise.resolve(this.body.slice(0));
  }

  clone() {
    return new CacheResponse(this.body.slice(0), this.ok, this.status);
  }
}

class MemoryCache {
  readonly entries = new Map<string, CacheResponse>();

  match(request: Request) {
    return Promise.resolve(this.entries.get(request.url));
  }

  put(request: Request, response: CacheResponse) {
    this.entries.set(request.url, response);
    return Promise.resolve();
  }
}

describe("noise model cache", () => {
  const cache = new MemoryCache();
  const fetchMock = vi.fn();

  beforeEach(() => {
    cache.entries.clear();
    fetchMock.mockReset();
    fetchMock.mockImplementation(async (request: Request) =>
      new CacheResponse(
        request.url.endsWith(".json")
          ? new TextEncoder().encode(JSON.stringify({ initialState: [0.25, 0.5] })).buffer
          : new Uint8Array([1, 2, 3]).buffer
      )
    );
    vi.stubGlobal("fetch", fetchMock);
    vi.stubGlobal("caches", {
      open: vi.fn(async () => cache)
    });
  });

  it("loads and caches the selected DPDFNet model", async () => {
    const bundle = await loadCachedNoiseModelBundle({
      ...defaultNoiseConfig,
      provider: "dpdfnet",
      dpdfnet: {
        model: "dpdfnet8_48khz_hr"
      }
    });

    expect(bundle?.provider).toBe("dpdfnet");
    expect(bundle?.models).toHaveLength(1);
    expect(bundle?.models[0]).toMatchObject({
      name: "dpdfnet8_48khz_hr",
      url: "/models/dpdfnet/dpdfnet8_48khz_hr.onnx"
    });
    expect(bundle?.provider === "dpdfnet" ? bundle.manifest.initialState : []).toEqual([0.25, 0.5]);
    expect(fetchMock).toHaveBeenCalledTimes(2);

    fetchMock.mockClear();
    await loadCachedNoiseModelBundle({
      ...defaultNoiseConfig,
      provider: "dpdfnet",
      dpdfnet: {
        model: "dpdfnet8_48khz_hr"
      }
    });

    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("loads the three DeepFilterNet model files", async () => {
    const bundle = await loadCachedNoiseModelBundle({
      ...defaultNoiseConfig,
      provider: "deepfilternet"
    });

    expect(bundle?.provider).toBe("deepfilternet");
    expect(bundle?.models.map((model) => model.url)).toEqual([
      "/models/deepfilternet/enc.onnx",
      "/models/deepfilternet/erb_dec.onnx",
      "/models/deepfilternet/df_dec.onnx"
    ]);
  });

  it("does not load models for RNNoise", async () => {
    await expect(
      loadCachedNoiseModelBundle({
        ...defaultNoiseConfig,
        provider: "rnnoise"
      })
    ).resolves.toBeNull();

    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("rejects non-48 kHz DPDFNet models for client-side processing", async () => {
    await expect(
      loadCachedNoiseModelBundle({
        ...defaultNoiseConfig,
        provider: "dpdfnet",
        dpdfnet: {
          model: "dpdfnet8"
        }
      })
    ).rejects.toThrow("client-side DPDFNet requires a 48 kHz model");
  });
});
