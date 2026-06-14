import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  answerServerMediaOffer,
  generatedMediaTopologyModeToRest,
  generatedMediaRelayModeToRest,
  generatedMediaRelayStatusToRest,
  generatedMediaTrackKindToRest,
  generatedNoiseProviderToRest,
  getIceServers,
  getMediaRelay,
  getMediaTopology,
  joinRoom,
  leaveRoom,
  mediaRelayUrl,
  registerMediaTrack,
  roomUrl,
  serverMediaOfferUrl,
  shareRoomUrl,
  startMediaRelay,
  stopMediaRelay,
  type JoinRoomResponse,
  type MediaRelayRoomStatus,
  type MediaTopology,
  type NoiseProvider,
  type ServerMediaAnswer
} from "./api";
import { MediaTopologyMode as WebrpcMediaTopologyMode } from "./lyre.gen";
import {
  MediaRelayMode as WebrpcMediaRelayMode,
  MediaRelayStatus as WebrpcMediaRelayStatus,
  MediaTrackKind as WebrpcMediaTrackKind,
  NoiseProvider as WebrpcNoiseProvider,
  ServerMediaSessionState,
  type JoinRoomResponse as WebrpcJoinRoomResponse,
  type MediaRelayRoomStatus as WebrpcMediaRelayRoomStatus,
  type ServerMediaAnswer as WebrpcServerMediaAnswer
} from "./lyre.gen";

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

const mediaTopologyFromGeneratedDerivedShape: MediaTopology = {
  mode: "p2p_mesh",
  turn_relay_supported: true,
  server_side_audio_processing: false,
  server_side_noise_cancelling: false,
  server_noise_cancelling_requires: "media_relay"
};
void mediaTopologyFromGeneratedDerivedShape;

const mediaRelayFromGeneratedDerivedShape: MediaRelayRoomStatus = {
  room_id: "DEFAULT",
  status: "inactive",
  mode: "p2p_mesh",
  server_side_audio_processing: false,
  server_side_noise_cancelling: false,
  noise: { provider: "off", intensity: 0.5, voice_activity_threshold: 0.35 },
  participants: [{ user_id: "user_a", tracks: [{ track_id: "audio-main", kind: "audio" }] }]
};
void mediaRelayFromGeneratedDerivedShape;

const generatedMediaRelayContract: WebrpcMediaRelayRoomStatus = {
  roomID: "DEFAULT",
  status: WebrpcMediaRelayStatus.INACTIVE,
  mode: WebrpcMediaRelayMode.P2P_MESH,
  serverSideAudioProcessing: false,
  serverSideNoiseCancelling: false,
  noise: { provider: WebrpcNoiseProvider.OFF, intensity: 0.5, voiceActivityThreshold: 0.35 },
  participants: [{ userID: "user_a", tracks: [{ trackID: "audio-main", kind: WebrpcMediaTrackKind.AUDIO }] }]
};
void generatedMediaRelayContract;

const serverMediaAnswerFromRestShape: ServerMediaAnswer = {
  room_id: "DEFAULT",
  user_id: "user_a",
  audio_track_id: "audio-main",
  sdp: "v=0",
  state: "negotiating"
};
void serverMediaAnswerFromRestShape;

const generatedServerMediaAnswerContract: WebrpcServerMediaAnswer = {
  roomID: "DEFAULT",
  userID: "user_a",
  audioTrackID: "audio-main",
  sdp: "v=0",
  state: ServerMediaSessionState.NEGOTIATING
};
void generatedServerMediaAnswerContract;

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

  it("builds encoded media relay urls", () => {
    expect(mediaRelayUrl("Team A")).toBe("https://api.example.test/api/rooms/Team%20A/media-relay");
  });

  it("builds encoded server media offer urls", () => {
    expect(serverMediaOfferUrl("Team A")).toBe(
      "https://api.example.test/api/rooms/Team%20A/server-media/offer"
    );
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

  it("maps generated topology mode values to REST topology strings", () => {
    expect(generatedMediaTopologyModeToRest(WebrpcMediaTopologyMode.P2P_MESH)).toBe("p2p_mesh");
    expect(generatedMediaTopologyModeToRest(WebrpcMediaTopologyMode.MEDIA_RELAY)).toBe("media_relay");
  });

  it("maps generated media relay values to REST strings", () => {
    expect(generatedMediaRelayStatusToRest(WebrpcMediaRelayStatus.INACTIVE)).toBe("inactive");
    expect(generatedMediaRelayStatusToRest(WebrpcMediaRelayStatus.ACTIVE)).toBe("active");
    expect(generatedMediaRelayModeToRest(WebrpcMediaRelayMode.P2P_MESH)).toBe("p2p_mesh");
    expect(generatedMediaRelayModeToRest(WebrpcMediaRelayMode.MEDIA_RELAY)).toBe("media_relay");
    expect(generatedMediaTrackKindToRest(WebrpcMediaTrackKind.AUDIO)).toBe("audio");
    expect(generatedMediaTrackKindToRest(WebrpcMediaTrackKind.VIDEO)).toBe("video");
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

  it("fetches media topology from API", async () => {
    await getMediaTopology();

    expect(fetch).toHaveBeenCalledWith("https://api.example.test/api/webrtc/topology");
  });

  it("fetches media relay status from API", async () => {
    await getMediaRelay("DEFAULT");

    expect(fetch).toHaveBeenCalledWith("https://api.example.test/api/rooms/DEFAULT/media-relay");
  });

  it("serializes media relay start request body", async () => {
    const noise = { provider: "rnnoise" as const, intensity: 0.8, voice_activity_threshold: 0.2 };
    await startMediaRelay("DEFAULT", noise);

    expect(fetch).toHaveBeenCalledWith("https://api.example.test/api/rooms/DEFAULT/media-relay/start", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ noise })
    });
  });

  it("serializes media relay stop request body", async () => {
    await stopMediaRelay("DEFAULT", "user_a");

    expect(fetch).toHaveBeenCalledWith("https://api.example.test/api/rooms/DEFAULT/media-relay/stop", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ user_id: "user_a" })
    });
  });

  it("serializes media relay track registration request body", async () => {
    await registerMediaTrack("DEFAULT", "user_a", "audio-main", "audio");

    expect(fetch).toHaveBeenCalledWith("https://api.example.test/api/rooms/DEFAULT/media-relay/tracks", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ user_id: "user_a", track_id: "audio-main", kind: "audio" })
    });
  });

  it("serializes server media offer request body", async () => {
    await answerServerMediaOffer("DEFAULT", "user_a", "audio-main", "v=0");

    expect(fetch).toHaveBeenCalledWith("https://api.example.test/api/rooms/DEFAULT/server-media/offer", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ user_id: "user_a", audio_track_id: "audio-main", sdp: "v=0" })
    });
  });
});
