import { beforeEach, describe, expect, it, vi } from "vitest";
import { defaultNoiseConfig, resetSettingsStoreForTests, useSettingsStore } from "./settings-store";
import { createAudioPeerConnection, createPeerConnection, openLocalAudioStream } from "./webrtc";

const clientNoise = vi.hoisted(() => ({
  processLocalAudioStream: vi.fn()
}));

vi.mock("./client-noise-cancellation", () => ({
  processLocalAudioStream: clientNoise.processLocalAudioStream
}));

describe("webrtc", () => {
  const addTrack = vi.fn();
  const peerConstructor = vi.fn();
  const stream = {
    getAudioTracks: () => [{ id: "track" }]
  } as unknown as MediaStream;

  class MockPeerConnection {
    addTrack = addTrack;

    constructor(config: RTCConfiguration) {
      peerConstructor(config);
    }
  }

  beforeEach(() => {
    localStorage.clear();
    resetSettingsStoreForTests();
    addTrack.mockClear();
    peerConstructor.mockClear();
    clientNoise.processLocalAudioStream.mockReset();
    clientNoise.processLocalAudioStream.mockImplementation(async (input: MediaStream) => input);
    vi.stubGlobal("RTCPeerConnection", MockPeerConnection);
    Object.defineProperty(navigator, "mediaDevices", {
      configurable: true,
      value: {
        getUserMedia: vi.fn(async () => stream)
      }
    });
  });

  it("opens one local audio stream", async () => {
    await expect(openLocalAudioStream()).resolves.toBe(stream);
    expect(navigator.mediaDevices.getUserMedia).toHaveBeenCalledWith({
      audio: {
        echoCancellation: true,
        autoGainControl: true,
        noiseSuppression: true
      }
    });
  });

  it("uses stored browser audio processing constraints", async () => {
    useSettingsStore.getState().setAudioProcessing({
      echoCancellation: false,
      autoGainControl: true,
      noiseSuppression: true,
      clientNoiseCancellation: false
    });

    await expect(openLocalAudioStream()).resolves.toBe(stream);

    expect(navigator.mediaDevices.getUserMedia).toHaveBeenCalledWith({
      audio: {
        echoCancellation: { exact: false },
        autoGainControl: true,
        noiseSuppression: true
      }
    });
  });

  it("wraps the microphone stream when client noise cancellation is enabled", async () => {
    const processedStream = {
      getAudioTracks: () => [{ id: "processed-track" }]
    } as unknown as MediaStream;
    clientNoise.processLocalAudioStream.mockResolvedValue(processedStream);
    useSettingsStore.getState().setAudioProcessing({
      echoCancellation: true,
      autoGainControl: true,
      noiseSuppression: true,
      clientNoiseCancellation: true
    });

    await expect(openLocalAudioStream()).resolves.toBe(processedStream);

    expect(clientNoise.processLocalAudioStream).toHaveBeenCalledWith(stream, {
      noise: defaultNoiseConfig
    });
  });

  it("passes the selected server denoise model into client noise cancellation", async () => {
    const selectedNoise = {
      ...defaultNoiseConfig,
      provider: "dpdfnet" as const,
      dpdfnet: {
        model: "dpdfnet8_48khz_hr"
      }
    };
    useSettingsStore.getState().setNoise(selectedNoise);
    useSettingsStore.getState().setAudioProcessing({
      echoCancellation: true,
      autoGainControl: true,
      noiseSuppression: true,
      clientNoiseCancellation: true
    });

    await expect(openLocalAudioStream()).resolves.toBe(stream);

    expect(clientNoise.processLocalAudioStream).toHaveBeenCalledWith(stream, {
      noise: selectedNoise
    });
  });

  it("keeps raw microphone stream when client noise cancellation cannot initialize", async () => {
    clientNoise.processLocalAudioStream.mockRejectedValue(new Error("missing wasm"));
    useSettingsStore.getState().setAudioProcessing({
      echoCancellation: true,
      autoGainControl: true,
      noiseSuppression: true,
      clientNoiseCancellation: true
    });

    await expect(openLocalAudioStream()).resolves.toBe(stream);
  });

  it("uses the stored microphone device when opening local audio", async () => {
    useSettingsStore.getState().setAudioDevices({
      inputDeviceId: "mic-a",
      outputDeviceId: ""
    });

    await expect(openLocalAudioStream()).resolves.toBe(stream);

    expect(navigator.mediaDevices.getUserMedia).toHaveBeenCalledWith({
      audio: {
        deviceId: { exact: "mic-a" },
        echoCancellation: true,
        autoGainControl: true,
        noiseSuppression: true
      }
    });
  });

  it("omits the microphone device constraint when default input is selected", async () => {
    useSettingsStore.getState().setAudioDevices({
      inputDeviceId: "",
      outputDeviceId: "speaker-a"
    });

    await expect(openLocalAudioStream()).resolves.toBe(stream);

    expect(navigator.mediaDevices.getUserMedia).toHaveBeenCalledWith({
      audio: {
        echoCancellation: true,
        autoGainControl: true,
        noiseSuppression: true
      }
    });
  });

  it("constructs peer connection with configured ice servers and local tracks", () => {
    createPeerConnection([{ urls: ["stun:stun.example:3478"], username: null, credential: null }], stream);

    expect(peerConstructor).toHaveBeenCalledWith({
      iceServers: [{ urls: ["stun:stun.example:3478"], username: undefined, credential: undefined }]
    });
    expect(addTrack).toHaveBeenCalledWith({ id: "track" }, stream);
  });

  it("keeps compatibility helper for one-off audio peer connection", async () => {
    await createAudioPeerConnection([{ urls: ["stun:stun.example:3478"], username: null, credential: null }]);

    expect(navigator.mediaDevices.getUserMedia).toHaveBeenCalledWith({
      audio: {
        echoCancellation: true,
        autoGainControl: true,
        noiseSuppression: true
      }
    });
    expect(peerConstructor).toHaveBeenCalledOnce();
    expect(addTrack).toHaveBeenCalledWith({ id: "track" }, stream);
  });
});
