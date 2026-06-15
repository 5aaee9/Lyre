import { beforeEach, describe, expect, it } from "vitest";
import { encodeAnswer, encodeIceCandidate, encodeOffer, reducePresence, roomSocketUrl } from "./signalling";

describe("signalling", () => {
  beforeEach(() => {
    window.__LYRE_CONFIG__ = {
      appApiUrl: "https://api.example.test",
      appBaseUrl: "https://app.example.test"
    };
  });

  it("derives websocket url from APP_API_URL", () => {
    expect(roomSocketUrl("Team A", "user_a", "token a")).toBe(
      "wss://api.example.test/api/rooms/Team%20A/ws?user_id=user_a&access_token=token+a"
    );
  });

  it("encodes offer answer and ice messages", () => {
    expect(encodeOffer("DEFAULT", "user_a", "offer").type).toBe("offer");
    expect(encodeAnswer("DEFAULT", "user_a", "answer", "user_b").recipient_id).toBe("user_b");
    expect(
      encodeIceCandidate("DEFAULT", "user_a", { candidate: "candidate", sdpMid: "0", sdpMLineIndex: 0 }).payload
    ).toEqual({ type: "ice-candidate", candidate: "candidate", sdp_mid: "0", sdp_m_line_index: 0 });
  });

  it("reduces presence messages", () => {
    const room = { room_id: "DEFAULT", users: [] };
    const joinedUser = {
      id: "user_a",
      nickname: "Ada",
      joined_at: new Date().toISOString(),
      noise: { provider: "off" as const, intensity: 0.5, voice_activity_threshold: 0.35 }
    };
    let state = reducePresence(
      {},
      { type: "room-snapshot", room_id: "DEFAULT", sender_id: "user_a", payload: { type: "room-snapshot", room } }
    );
    state = reducePresence(state, {
      type: "user-joined",
      room_id: "DEFAULT",
      sender_id: "user_a",
      payload: { type: "user-joined", user: joinedUser }
    });
    expect(state.room?.users).toHaveLength(1);

    state = reducePresence(state, {
      type: "user-left",
      room_id: "DEFAULT",
      sender_id: "user_a",
      payload: { type: "user-left", user_id: "user_a" }
    });
    expect(state.room?.users).toHaveLength(0);

    state = reducePresence(state, {
      type: "error",
      room_id: "DEFAULT",
      sender_id: "user_a",
      payload: { type: "error", message: "bad" }
    });
    expect(state.error).toBe("bad");
  });
});
