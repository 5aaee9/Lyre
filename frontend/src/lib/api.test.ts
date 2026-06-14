import { beforeEach, describe, expect, it, vi } from "vitest";
import { getIceServers, joinRoom, leaveRoom, roomUrl, shareRoomUrl } from "./api";

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
