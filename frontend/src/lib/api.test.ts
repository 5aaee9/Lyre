import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  generatedNoiseProviderToRest,
  getIceServers,
  joinRoom,
  leaveRoom,
  roomUrl,
  shareRoomUrl,
  type JoinRoomResponse,
  type NoiseProvider
} from "./api";
import { NoiseProvider as WebrpcNoiseProvider, type JoinRoomResponse as WebrpcJoinRoomResponse } from "./lyre.gen";

const providerFromGenerated: NoiseProvider = generatedNoiseProviderToRest(WebrpcNoiseProvider.OFF);
void providerFromGenerated;

const joinResponseFromGeneratedDerivedShape: JoinRoomResponse = {
  user: {
    id: "user_a",
    nickname: "Ada",
    joined_at: "2026-06-14T00:00:00Z",
    noise: { provider: "off", intensity: 0.5, voice_activity_threshold: 0.35 }
  },
  room: {
    room_id: "DEFAULT",
    users: []
  }
};
void joinResponseFromGeneratedDerivedShape;

const generatedJoinRoomResponseContract: WebrpcJoinRoomResponse = {
  user: {
    id: "user_a",
    nickname: "Ada",
    joinedAt: "2026-06-14T00:00:00Z",
    noise: { provider: WebrpcNoiseProvider.OFF, intensity: 0.5, voiceActivityThreshold: 0.35 }
  },
  room: {
    roomID: "DEFAULT",
    users: []
  }
};
void generatedJoinRoomResponseContract;

describe("api", () => {
  beforeEach(() => {
    window.__LYRE_CONFIG__ = {
      appApiUrl: "https://api.example.test",
      appBaseUrl: "https://app.example.test"
    };
    global.fetch = vi.fn(async () => new Response(JSON.stringify({ ok: true }))) as typeof fetch;
  });

  it("builds encoded room and share urls", () => {
    expect(roomUrl("Team A")).toBe("https://api.example.test/api/rooms/Team%20A");
    expect(shareRoomUrl("Team A")).toBe("https://app.example.test/room/Team%20A");
  });

  it("serializes join request body", async () => {
    const noise = { provider: "rnnoise" as const, intensity: 0.8, voice_activity_threshold: 0.15 };
    await joinRoom("DEFAULT", { nickname: "Ada", noise });

    expect(fetch).toHaveBeenCalledWith("https://api.example.test/api/rooms/DEFAULT/join", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ nickname: "Ada", noise })
    });
  });

  it("maps generated noise provider values to REST provider strings", () => {
    expect(generatedNoiseProviderToRest(WebrpcNoiseProvider.OFF)).toBe("off");
    expect(generatedNoiseProviderToRest(WebrpcNoiseProvider.RNNOISE)).toBe("rnnoise");
    expect(generatedNoiseProviderToRest(WebrpcNoiseProvider.DEEPFILTERNET)).toBe("deepfilternet");
  });

  it("serializes leave request body", async () => {
    await leaveRoom("DEFAULT", "user_a");

    expect(fetch).toHaveBeenCalledWith("https://api.example.test/api/rooms/DEFAULT/leave", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ user_id: "user_a" })
    });
  });

  it("fetches ice servers from API", async () => {
    await getIceServers();

    expect(fetch).toHaveBeenCalledWith("https://api.example.test/api/webrtc/ice-servers");
  });
});
