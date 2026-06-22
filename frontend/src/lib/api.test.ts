import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  addServerMediaIceCandidate,
  answerServerMediaOffer,
  closeServerMediaSession,
  generatedMediaTopologyModeToRest,
  generatedMediaRelayModeToRest,
  generatedMediaRelayStatusToRest,
  generatedMediaTrackKindToRest,
  generatedNoiseProviderToRest,
  getIceServers,
  getMediaRelay,
  getMediaTopology,
  getServerMediaIceCandidates,
  joinRoom,
  leaveRoom,
  mediaRelayUrl,
  registerMediaParticipant,
  registerMediaTrack,
  roomUrl,
  serverMediaCandidatesUrl,
  serverMediaCloseUrl,
  serverMediaOfferUrl,
  shareRoomUrl,
  startMediaRelay,
  stopMediaRelay,
  updateMediaRelaySettings,
  updateMediaRelaySubscriptions,
  type JoinRoomResponse,
  type MediaRelayRoomStatus,
  type MediaRelaySubscriptions,
  type MediaTopology,
  type NoiseProvider,
  type ServerMediaAnswer,
  type ServerMediaIceCandidate,
  type CloseServerMediaSessionResponse
} from "./api";
import { MediaTopologyMode as WebrpcMediaTopologyMode } from "./lyre.gen";
import {
  MediaRelayMode as WebrpcMediaRelayMode,
  MediaRelayStatus as WebrpcMediaRelayStatus,
  MediaTrackKind as WebrpcMediaTrackKind,
  NoiseProvider as WebrpcNoiseProvider,
  ServerMediaSessionState,
  type ClosedServerMediaSession as WebrpcClosedServerMediaSession,
  type CloseServerMediaSessionResponse as WebrpcCloseServerMediaSessionResponse,
  type JoinRoomResponse as WebrpcJoinRoomResponse,
  type MediaRelayRoomStatus as WebrpcMediaRelayRoomStatus,
  type RegisterMediaParticipantRequest as WebrpcRegisterMediaParticipantRequest,
  type RegisterMediaParticipantResponse as WebrpcRegisterMediaParticipantResponse,
  type ServerMediaAnswer as WebrpcServerMediaAnswer,
  type ServerMediaIceCandidate as WebrpcServerMediaIceCandidate
} from "./lyre.gen";
import { defaultNoiseConfig } from "./settings-store";

const providerFromGenerated: NoiseProvider = generatedNoiseProviderToRest(WebrpcNoiseProvider.OFF);
void providerFromGenerated;

const joinResponseFromGeneratedDerivedShape: JoinRoomResponse = {
  access_token: "token_a",
  user: {
    id: "user_a",
    nickname: "Ada",
    joined_at: "2026-06-14T00:00:00Z",
    noise: { provider: "off", intensity: 0.5, voice_activity_threshold: 0.35, dpdfnet: defaultNoiseConfig.dpdfnet }
  },
  room: {
    room_id: "DEFAULT",
    users: []
  }
};
void joinResponseFromGeneratedDerivedShape;

const generatedJoinRoomResponseContract: WebrpcJoinRoomResponse = {
  accessToken: "token_a",
  user: {
    id: "user_a",
    nickname: "Ada",
    joinedAt: "2026-06-14T00:00:00Z",
    noise: {
      provider: WebrpcNoiseProvider.OFF,
      intensity: 0.5,
      voiceActivityThreshold: 0.35,
      dpdfnet: defaultNoiseConfig.dpdfnet
    }
  },
  room: {
    roomID: "DEFAULT",
    users: []
  }
};
void generatedJoinRoomResponseContract;

const mediaTopologyFromGeneratedDerivedShape: MediaTopology = {
  mode: "media_relay",
  turn_relay_supported: true,
  server_side_audio_processing: true,
  server_side_noise_cancelling: true,
  server_noise_cancelling_requires: "media_relay"
};
void mediaTopologyFromGeneratedDerivedShape;

const mediaRelayFromGeneratedDerivedShape: MediaRelayRoomStatus = {
  room_id: "DEFAULT",
  status: "inactive",
  mode: "media_relay",
  server_side_audio_processing: false,
  server_side_noise_cancelling: false,
  noise: { provider: "off", intensity: 0.5, voice_activity_threshold: 0.35, dpdfnet: defaultNoiseConfig.dpdfnet },
  participants: [{ user_id: "user_a", tracks: [{ track_id: "audio-main", kind: "audio" }] }]
};
void mediaRelayFromGeneratedDerivedShape;

const mediaRelaySubscriptionsFromGeneratedDerivedShape: MediaRelaySubscriptions = {
  room_id: "DEFAULT",
  user_id: "user_a",
  source_user_ids: ["user_b"]
};
void mediaRelaySubscriptionsFromGeneratedDerivedShape;

const generatedMediaRelayContract: WebrpcMediaRelayRoomStatus = {
  roomID: "DEFAULT",
  status: WebrpcMediaRelayStatus.INACTIVE,
  mode: WebrpcMediaRelayMode.MEDIA_RELAY,
  serverSideAudioProcessing: false,
  serverSideNoiseCancelling: false,
  noise: {
    provider: WebrpcNoiseProvider.OFF,
    intensity: 0.5,
    voiceActivityThreshold: 0.35,
    dpdfnet: defaultNoiseConfig.dpdfnet
  },
  participants: [{ userID: "user_a", tracks: [{ trackID: "audio-main", kind: WebrpcMediaTrackKind.AUDIO }] }]
};
void generatedMediaRelayContract;

const generatedRegisterMediaParticipantRequestContract: WebrpcRegisterMediaParticipantRequest = {
  roomID: "DEFAULT",
  userID: "user_a"
};
void generatedRegisterMediaParticipantRequestContract;

const generatedRegisterMediaParticipantResponseContract: WebrpcRegisterMediaParticipantResponse = {
  mediaRelay: generatedMediaRelayContract
};
void generatedRegisterMediaParticipantResponseContract;

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

const serverMediaCandidateFromRestShape: ServerMediaIceCandidate = {
  room_id: "DEFAULT",
  user_id: "user_a",
  candidate: "candidate:1",
  sdp_mid: "0",
  sdp_mline_index: 0,
  username_fragment: null
};
void serverMediaCandidateFromRestShape;

const generatedServerMediaCandidateContract: WebrpcServerMediaIceCandidate = {
  roomID: "DEFAULT",
  userID: "user_a",
  candidate: "candidate:1",
  sdpMid: "0",
  sdpMLineIndex: 0,
  usernameFragment: undefined
};
void generatedServerMediaCandidateContract;

const closeServerMediaSessionFromRestShape: CloseServerMediaSessionResponse = {
  media_relay: mediaRelayFromGeneratedDerivedShape,
  session: {
    room_id: "DEFAULT",
    user_id: "user_a",
    audio_track_id: "audio-main",
    state: "closed"
  }
};
void closeServerMediaSessionFromRestShape;

const generatedClosedServerMediaSessionPayload: WebrpcClosedServerMediaSession = {
  mediaRelay: generatedMediaRelayContract,
  session: {
    roomID: "DEFAULT",
    userID: "user_a",
    audioTrackID: "audio-main",
    state: ServerMediaSessionState.CLOSED
  }
};
void generatedClosedServerMediaSessionPayload;

const generatedCloseServerMediaSessionContract: WebrpcCloseServerMediaSessionResponse = {
  closed: generatedClosedServerMediaSessionPayload
};
void generatedCloseServerMediaSessionContract;

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

  it("builds encoded server media candidate urls", () => {
    expect(serverMediaCandidatesUrl("Team A")).toBe(
      "https://api.example.test/api/rooms/Team%20A/server-media/candidates"
    );
  });

  it("builds encoded server media close urls", () => {
    expect(serverMediaCloseUrl("Team A")).toBe(
      "https://api.example.test/api/rooms/Team%20A/server-media/close"
    );
  });

  it("serializes join request body", async () => {
    const noise = {
      provider: "rnnoise" as const,
      intensity: 0.8,
      voice_activity_threshold: 0.15,
      dpdfnet: defaultNoiseConfig.dpdfnet
    };
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
    expect(generatedMediaTopologyModeToRest(WebrpcMediaTopologyMode.MEDIA_RELAY)).toBe("media_relay");
  });

  it("maps generated media relay values to REST strings", () => {
    expect(generatedMediaRelayStatusToRest(WebrpcMediaRelayStatus.INACTIVE)).toBe("inactive");
    expect(generatedMediaRelayStatusToRest(WebrpcMediaRelayStatus.ACTIVE)).toBe("active");
    expect(generatedMediaRelayModeToRest(WebrpcMediaRelayMode.MEDIA_RELAY)).toBe("media_relay");
    expect(generatedMediaTrackKindToRest(WebrpcMediaTrackKind.AUDIO)).toBe("audio");
    expect(generatedMediaTrackKindToRest(WebrpcMediaTrackKind.VIDEO)).toBe("video");
  });

  it("serializes leave request body", async () => {
    await leaveRoom("DEFAULT", "user_a", "token_a");

    expect(fetch).toHaveBeenCalledWith("https://api.example.test/api/rooms/DEFAULT/leave", {
      method: "POST",
      headers: { authorization: "Bearer token_a", "content-type": "application/json" },
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
    const noise = {
      provider: "rnnoise" as const,
      intensity: 0.8,
      voice_activity_threshold: 0.2,
      dpdfnet: defaultNoiseConfig.dpdfnet
    };
    await startMediaRelay("DEFAULT", noise, "token_a");

    expect(fetch).toHaveBeenCalledWith("https://api.example.test/api/rooms/DEFAULT/media-relay/start", {
      method: "POST",
      headers: { authorization: "Bearer token_a", "content-type": "application/json" },
      body: JSON.stringify({ noise })
    });
  });

  it("serializes media relay stop request body", async () => {
    await stopMediaRelay("DEFAULT", "user_a", "token_a");

    expect(fetch).toHaveBeenCalledWith("https://api.example.test/api/rooms/DEFAULT/media-relay/stop", {
      method: "POST",
      headers: { authorization: "Bearer token_a", "content-type": "application/json" },
      body: JSON.stringify({ user_id: "user_a" })
    });
  });

  it("serializes media relay settings updates", async () => {
    const noise = {
      provider: "off" as const,
      intensity: 0.5,
      voice_activity_threshold: 0.35,
      dpdfnet: defaultNoiseConfig.dpdfnet
    };
    await updateMediaRelaySettings("DEFAULT", "user_a", noise, "token_a");

    expect(fetch).toHaveBeenCalledWith("https://api.example.test/api/rooms/DEFAULT/media-relay/settings", {
      method: "POST",
      headers: { authorization: "Bearer token_a", "content-type": "application/json" },
      body: JSON.stringify({ user_id: "user_a", noise })
    });
  });

  it("serializes media relay track registration request body", async () => {
    await registerMediaTrack("DEFAULT", "user_a", "audio-main", "audio", "token_a");

    expect(fetch).toHaveBeenCalledWith("https://api.example.test/api/rooms/DEFAULT/media-relay/tracks", {
      method: "POST",
      headers: { authorization: "Bearer token_a", "content-type": "application/json" },
      body: JSON.stringify({ user_id: "user_a", track_id: "audio-main", kind: "audio" })
    });
  });

  it("serializes media relay participant registration request body", async () => {
    await registerMediaParticipant("DEFAULT", "user_a", "token_a");

    expect(fetch).toHaveBeenCalledWith("https://api.example.test/api/rooms/DEFAULT/media-relay/participants", {
      method: "POST",
      headers: { authorization: "Bearer token_a", "content-type": "application/json" },
      body: JSON.stringify({ user_id: "user_a" })
    });
  });

  it("serializes media relay subscription updates", async () => {
    await updateMediaRelaySubscriptions("DEFAULT", "user_a", ["user_b"], "token_a");

    expect(fetch).toHaveBeenCalledWith("https://api.example.test/api/rooms/DEFAULT/media-relay/subscriptions", {
      method: "POST",
      headers: { authorization: "Bearer token_a", "content-type": "application/json" },
      body: JSON.stringify({ user_id: "user_a", source_user_ids: ["user_b"] })
    });
  });

  it("serializes server media offer request body", async () => {
    await answerServerMediaOffer("DEFAULT", "user_a", "audio-main", "v=0", "token_a");

    expect(fetch).toHaveBeenCalledWith("https://api.example.test/api/rooms/DEFAULT/server-media/offer", {
      method: "POST",
      headers: { authorization: "Bearer token_a", "content-type": "application/json" },
      body: JSON.stringify({ user_id: "user_a", audio_track_id: "audio-main", sdp: "v=0" })
    });
  });

  it("serializes server media ICE candidate request body", async () => {
    await addServerMediaIceCandidate(
      "DEFAULT",
      {
        user_id: "user_a",
        candidate: "candidate:1",
        sdp_mid: "0",
        sdp_mline_index: 0,
        username_fragment: null
      },
      "token_a"
    );

    expect(fetch).toHaveBeenCalledWith("https://api.example.test/api/rooms/DEFAULT/server-media/candidates", {
      method: "POST",
      headers: { authorization: "Bearer token_a", "content-type": "application/json" },
      body: JSON.stringify({
        user_id: "user_a",
        candidate: "candidate:1",
        sdp_mid: "0",
        sdp_mline_index: 0,
        username_fragment: null
      })
    });
  });

  it("fetches server media ICE candidates with encoded user id", async () => {
    await getServerMediaIceCandidates("DEFAULT", "user a", "token_a");

    expect(fetch).toHaveBeenCalledWith("https://api.example.test/api/rooms/DEFAULT/server-media/candidates?user_id=user+a", {
      headers: { authorization: "Bearer token_a" }
    });
  });

  it("serializes server media close request body", async () => {
    await closeServerMediaSession("DEFAULT", "user_a", "token_a");

    expect(fetch).toHaveBeenCalledWith("https://api.example.test/api/rooms/DEFAULT/server-media/close", {
      method: "POST",
      headers: { authorization: "Bearer token_a", "content-type": "application/json" },
      body: JSON.stringify({ user_id: "user_a" })
    });
  });

  it("throws useful errors for failed server media flow responses", async () => {
    global.fetch = vi.fn(async () => new Response(JSON.stringify({ error: "nope" }), { status: 503 })) as typeof fetch;

    await expect(startMediaRelay("DEFAULT", undefined, "token_a")).rejects.toThrow("failed to start media relay: 503");
    await expect(registerMediaTrack("DEFAULT", "user_a", "audio-main", "audio", "token_a")).rejects.toThrow(
      "failed to register media track: 503"
    );
    await expect(registerMediaParticipant("DEFAULT", "user_a", "token_a")).rejects.toThrow(
      "failed to register media participant: 503"
    );
    await expect(updateMediaRelaySettings("DEFAULT", "user_a", defaultNoiseConfig, "token_a")).rejects.toThrow(
      "failed to update media relay settings: 503"
    );
    await expect(updateMediaRelaySubscriptions("DEFAULT", "user_a", ["user_b"], "token_a")).rejects.toThrow(
      "failed to update media relay subscriptions: 503"
    );
    await expect(answerServerMediaOffer("DEFAULT", "user_a", "audio-main", "v=0", "token_a")).rejects.toThrow(
      "failed to negotiate server media offer: 503"
    );
    await expect(
      addServerMediaIceCandidate(
        "DEFAULT",
        {
          user_id: "user_a",
          candidate: "candidate:1",
          sdp_mid: "0",
          sdp_mline_index: 0,
          username_fragment: null
        },
        "token_a"
      )
    ).rejects.toThrow("failed to add server media ICE candidate: 503");
    await expect(getServerMediaIceCandidates("DEFAULT", "user_a", "token_a")).rejects.toThrow(
      "failed to load server media ICE candidates: 503"
    );
    await expect(closeServerMediaSession("DEFAULT", "user_a", "token_a")).rejects.toThrow(
      "failed to close server media session: 503"
    );
  });
});
