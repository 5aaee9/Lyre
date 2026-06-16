import { beforeEach, describe, expect, it } from "vitest";
import {
  encodeAnswer,
  encodeIceCandidate,
  encodeOffer,
  encodeServerMediaIceCandidate,
  encodeServerMediaIceCandidatesRequest,
  reducePresence,
  roomSocketUrl
} from "./signalling";
import { defaultNoiseConfig } from "./settings-store";

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

  it("encodes server-media ice candidate websocket messages", () => {
    expect(
      encodeServerMediaIceCandidate("DEFAULT", "user_a", {
        candidate: "candidate:local",
        sdpMid: "0",
        sdpMLineIndex: 0,
        usernameFragment: "ufrag"
      })
    ).toEqual({
      type: "server-media-ice-candidate",
      room_id: "DEFAULT",
      sender_id: "user_a",
      recipient_id: "user_a",
      payload: {
        type: "server-media-ice-candidate",
        candidate: "candidate:local",
        sdp_mid: "0",
        sdp_mline_index: 0,
        username_fragment: "ufrag"
      }
    });
  });

  it("encodes server-media ice candidates request websocket messages", () => {
    expect(encodeServerMediaIceCandidatesRequest("DEFAULT", "user_a")).toEqual({
      type: "server-media-ice-candidates-request",
      room_id: "DEFAULT",
      sender_id: "user_a",
      recipient_id: "user_a",
      payload: { type: "server-media-ice-candidates-request" }
    });
  });

  it("reduces presence messages", () => {
    const room = { room_id: "DEFAULT", users: [] };
    const joinedUser = {
      id: "user_a",
      nickname: "Ada",
      joined_at: new Date().toISOString(),
      noise: {
        provider: "off" as const,
        intensity: 0.5,
        voice_activity_threshold: 0.35,
        dpdfnet: defaultNoiseConfig.dpdfnet
      }
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

    state = reducePresence(state, {
      type: "server-media-ice-candidates",
      room_id: "DEFAULT",
      sender_id: "user_a",
      recipient_id: "user_a",
      payload: { type: "server-media-ice-candidates", candidates: [] }
    });
    expect(state.room?.users).toHaveLength(0);
    expect(state.error).toBe("bad");
  });
});
