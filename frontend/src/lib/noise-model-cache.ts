import type { NoiseCancellationConfig } from "./api";

export type CachedNoiseModel = {
  name: string;
  url: string;
  bytes: ArrayBuffer;
};

export type CachedDpdfNetManifest = {
  initialState: number[];
};

export type CachedDeepFilterNetModelBundle = {
  provider: "deepfilternet";
  models: CachedNoiseModel[];
};

export type CachedDpdfNetModelBundle = {
  provider: "dpdfnet";
  manifest: CachedDpdfNetManifest;
  models: CachedNoiseModel[];
};

export type CachedNoiseModelBundle = CachedDeepFilterNetModelBundle | CachedDpdfNetModelBundle;

const CACHE_NAME = "lyre.noise-models.v1";

export async function loadCachedNoiseModelBundle(
  noise: NoiseCancellationConfig
): Promise<CachedNoiseModelBundle | null> {
  if (noise.provider === "deepfilternet") {
    return {
      provider: "deepfilternet",
      models: await Promise.all([
        cachedModel("enc", "/models/deepfilternet/enc.onnx"),
        cachedModel("erb_dec", "/models/deepfilternet/erb_dec.onnx"),
        cachedModel("df_dec", "/models/deepfilternet/df_dec.onnx")
      ])
    };
  }
  if (noise.provider === "dpdfnet") {
    if (!noise.dpdfnet.model.includes("48khz")) {
      throw new Error(`client-side DPDFNet requires a 48 kHz model, got ${noise.dpdfnet.model}`);
    }
    const modelUrl = `/models/dpdfnet/${noise.dpdfnet.model}.onnx`;
    const manifestUrl = `/models/dpdfnet/${noise.dpdfnet.model}.json`;
    const [model, manifest] = await Promise.all([
      cachedModel(noise.dpdfnet.model, modelUrl),
      cachedDpdfNetManifest(manifestUrl)
    ]);
    return {
      provider: "dpdfnet",
      manifest,
      models: [model]
    };
  }
  return null;
}

async function cachedModel(name: string, url: string): Promise<CachedNoiseModel> {
  return {
    name,
    url,
    bytes: await cachedBytes(url)
  };
}

async function cachedDpdfNetManifest(url: string): Promise<CachedDpdfNetManifest> {
  const manifest = JSON.parse(new TextDecoder().decode(await cachedBytes(url))) as unknown;
  const initialState = (manifest as { initialState?: unknown }).initialState;
  if (!Array.isArray(initialState) || initialState.some((value) => typeof value !== "number")) {
    throw new Error(`invalid DPDFNet model manifest ${url}`);
  }
  return { initialState };
}

async function cachedBytes(url: string): Promise<ArrayBuffer> {
  const request = new Request(resolveModelUrl(url), { cache: "reload" });
  if (!("caches" in globalThis)) {
    return fetchBytes(request);
  }

  const cache = await caches.open(CACHE_NAME);
  const cached = await cache.match(request);
  if (cached) {
    return cached.arrayBuffer();
  }

  const response = await fetch(request);
  if (!response.ok) {
    throw new Error(`failed to load noise model ${url}: ${response.status}`);
  }
  await cache.put(request, response.clone());
  return response.arrayBuffer();
}

function resolveModelUrl(url: string): string {
  if (typeof window === "undefined") {
    return new URL(url, "http://localhost").toString();
  }
  return new URL(url, window.location.origin).toString();
}

async function fetchBytes(request: Request): Promise<ArrayBuffer> {
  const response = await fetch(request);
  if (!response.ok) {
    throw new Error(`failed to load noise model ${request.url}: ${response.status}`);
  }
  return response.arrayBuffer();
}
