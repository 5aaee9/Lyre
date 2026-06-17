import { beforeEach, describe, expect, it, vi } from "vitest";
import { resetSettingsStoreForTests, useSettingsStore } from "./settings-store";
import { createAudioPeerConnection, createPeerConnection, openLocalAudioStream } from "./webrtc";

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
      noiseSuppression: true
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
