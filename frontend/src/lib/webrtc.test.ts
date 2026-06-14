import { beforeEach, describe, expect, it, vi } from "vitest";
import { createAudioPeerConnection } from "./webrtc";

describe("webrtc", () => {
  const addTrack = vi.fn();
  const peerConstructor = vi.fn();

  class MockPeerConnection {
    addTrack = addTrack;

    constructor(config: RTCConfiguration) {
      peerConstructor(config);
    }
  }

  beforeEach(() => {
    addTrack.mockClear();
    peerConstructor.mockClear();
    vi.stubGlobal("RTCPeerConnection", MockPeerConnection);
    Object.defineProperty(navigator, "mediaDevices", {
      configurable: true,
      value: {
        getUserMedia: vi.fn(async () => ({
          getAudioTracks: () => [{ id: "track" }]
        }))
      }
    });
  });

  it("constructs peer connection with configured ice servers", async () => {
    await createAudioPeerConnection([
      { urls: ["stun:stun.example:3478"], username: null, credential: null }
    ]);

    expect(peerConstructor).toHaveBeenCalledWith({
      iceServers: [{ urls: ["stun:stun.example:3478"], username: undefined, credential: undefined }]
    });
    expect(navigator.mediaDevices.getUserMedia).toHaveBeenCalledWith({ audio: true });
    expect(addTrack).toHaveBeenCalled();
  });
});
