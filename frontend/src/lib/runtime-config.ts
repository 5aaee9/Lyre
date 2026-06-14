export type RuntimeConfig = {
  appBaseUrl: string;
  appApiUrl: string;
};

declare global {
  interface Window {
    __LYRE_CONFIG__?: RuntimeConfig;
  }
}

export function runtimeConfig(): RuntimeConfig {
  if (typeof window !== "undefined" && window.__LYRE_CONFIG__) {
    return window.__LYRE_CONFIG__;
  }
  return {
    appBaseUrl: "http://localhost:3000",
    appApiUrl: "http://localhost:8080"
  };
}
