import { afterEach, beforeEach, vi } from "vitest";
import type { UserProfile } from "@/lib/api";
import { defaultNoiseConfig, resetSettingsStoreForTests } from "@/lib/settings-store";

const send = vi.fn();
const sockets: MockWebSocket[] = [];
const getUserMedia = vi.fn();
const stopTrack = vi.fn();
const localAudioTrack = { id: "track", enabled: true, stop: stopTrack };
const addRemoteTrack = vi.fn();
const removeAudio = vi.fn();
const playAudio = vi.fn();
const gainNodes: MockGainNode[] = [];
const audioContexts: MockAudioContext[] = [];
const voiceActivityMock = vi.hoisted(() => {
  type MockVoiceActivityDetectorInstance = {
    stream: MediaStream;
    onSpeakingChange: (speaking: boolean) => void;
    start: ReturnType<typeof vi.fn>;
    stop: ReturnType<typeof vi.fn>;
  };
  const instances: MockVoiceActivityDetectorInstance[] = [];
  class MockVoiceActivityDetector {
    stream: MediaStream;
    onSpeakingChange: (speaking: boolean) => void;
    start = vi.fn();
    stop = vi.fn();

    constructor(stream: MediaStream, onSpeakingChange: (speaking: boolean) => void) {
      this.stream = stream;
      this.onSpeakingChange = onSpeakingChange;
      instances.push(this);
    }
  }
  return { MockVoiceActivityDetector, instances };
});
const apiMocks = vi.hoisted(() => ({
  answerServerMediaOffer: vi.fn(),
  closeServerMediaSession: vi.fn(),
  getMediaRelay: vi.fn(),
  getIceServers: vi.fn(async () => [{ urls: ["stun:stun.example:3478"], username: null, credential: null }]),
  leaveRoom: vi.fn(),
  registerMediaTrack: vi.fn(),
  startMediaRelay: vi.fn(),
  stopMediaRelay: vi.fn(),
  updateMediaRelaySettings: vi.fn(),
  updateMediaRelaySubscriptions: vi.fn()
}));
const peerConnections: MockPeerConnection[] = [];
const peerStatsReports: Map<string, unknown>[] = [];
const createOfferMock = vi.fn(async (peer: MockPeerConnection) => ({
  type: "offer",
  sdp: `local-offer-${peerConnections.indexOf(peer)}`
}));

function makeUser(id: string, nickname = id): UserProfile {
  return {
    id,
    nickname,
    joined_at: new Date().toISOString(),
    noise: defaultNoiseConfig
  };
}

const users = [makeUser("user_a", "Ada"), makeUser("user_b", "Bob"), makeUser("user_c", "Cam")];

class MockWebSocket {
  static readonly OPEN = 1;
  static readonly CLOSED = 3;
  readyState = MockWebSocket.OPEN;
  onopen: (() => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onclose: (() => void) | null = null;
  send = send;
  close = vi.fn();

  constructor() {
    sockets.push(this);
    setTimeout(() => this.onopen?.(), 0);
  }
}

class MockPeerConnection {
  connectionState: RTCPeerConnectionState = "connected";
  iceConnectionState: RTCIceConnectionState = "new";
  signalingState: RTCSignalingState = "stable";
  onicecandidate: ((event: RTCPeerConnectionIceEvent) => void) | null = null;
  oniceconnectionstatechange: (() => void) | null = null;
  ontrack: ((event: RTCTrackEvent) => void) | null = null;
  audioSender = {
    track: { kind: "audio" },
    getParameters: vi.fn(() => ({ encodings: [{}] })),
    setParameters: vi.fn(async () => undefined)
  };
  addTrack = vi.fn();
  addIceCandidate = vi.fn();
  close = vi.fn();
  createAnswer = vi.fn(async () => ({ type: "answer", sdp: `local-answer-${this.remoteUserId}` }));
  createOffer = vi.fn(async () => createOfferMock(this));
  getReceivers = vi.fn(() => [] as RTCRtpReceiver[]);
  getSenders = vi.fn(() => [this.audioSender]);
  getStats = vi.fn(async () => peerStatsReports[peerConnections.indexOf(this)] ?? new Map());
  setLocalDescription = vi.fn();
  setRemoteDescription = vi.fn();
  remoteUserId?: string;

  constructor() {
    peerConnections.push(this);
  }
}

class MockMediaStream {
  tracks: unknown[] = [];
  addTrack = addRemoteTrack;
  getAudioTracks = () => this.tracks;

  constructor() {
    this.addTrack = vi.fn((track: unknown) => {
      addRemoteTrack(track);
      this.tracks.push(track);
    });
  }
}

class MockGainNode {
  gain = { value: 1 };
  connect = vi.fn();
  disconnect = vi.fn();

  constructor() {
    gainNodes.push(this);
  }
}

class MockAudioSource {
  connect = vi.fn();
  disconnect = vi.fn();
}

class MockAudioContext {
  state = "suspended";
  destination = {};
  createMediaStreamSource = vi.fn(() => new MockAudioSource());
  createGain = vi.fn(() => new MockGainNode());
  close = vi.fn();
  resume = vi.fn(async () => {
    this.state = "running";
  });

  constructor() {
    audioContexts.push(this);
  }
}

vi.mock("@/lib/api", async () => {
  const actual = await vi.importActual<typeof import("@/lib/api")>("@/lib/api");
  return {
    ...actual,
    joinRoom: vi.fn(async () => ({
      access_token: "token_a",
      user: users[0],
      room: { room_id: "DEFAULT", users }
    })),
    getMediaRelay: apiMocks.getMediaRelay,
    getIceServers: apiMocks.getIceServers,
    leaveRoom: apiMocks.leaveRoom,
    startMediaRelay: apiMocks.startMediaRelay,
    stopMediaRelay: apiMocks.stopMediaRelay,
    registerMediaTrack: apiMocks.registerMediaTrack,
    answerServerMediaOffer: apiMocks.answerServerMediaOffer,
    closeServerMediaSession: apiMocks.closeServerMediaSession,
    updateMediaRelaySettings: apiMocks.updateMediaRelaySettings,
    updateMediaRelaySubscriptions: apiMocks.updateMediaRelaySubscriptions,
    shareRoomUrl: () => "http://localhost:3000/room/DEFAULT"
  };
});

vi.mock("@/lib/voice-activity", () => ({
  VoiceActivityDetector: voiceActivityMock.MockVoiceActivityDetector
}));

export {
  addRemoteTrack,
  apiMocks,
  audioContexts,
  gainNodes,
  getUserMedia,
  localAudioTrack,
  makeUser,
  peerConnections,
  peerStatsReports,
  playAudio,
  removeAudio,
  send,
  sockets,
  stopTrack,
  voiceActivityMock
};

afterEach(() => {
  vi.restoreAllMocks();
});

beforeEach(() => {
  sockets.length = 0;
  peerConnections.length = 0;
  peerStatsReports.length = 0;
  gainNodes.length = 0;
  audioContexts.length = 0;
  voiceActivityMock.instances.length = 0;
  localStorage.clear();
  sessionStorage.clear();
  resetSettingsStoreForTests();
  send.mockClear();
  getUserMedia.mockReset();
  localAudioTrack.enabled = true;
  stopTrack.mockClear();
  addRemoteTrack.mockClear();
  removeAudio.mockClear();
  playAudio.mockReset();
  playAudio.mockResolvedValue(undefined);
  createOfferMock.mockReset();
  createOfferMock.mockImplementation(async (peer: MockPeerConnection) => ({
    type: "offer",
    sdp: `local-offer-${peerConnections.indexOf(peer)}`
  }));
  apiMocks.getIceServers.mockClear();
  apiMocks.getIceServers.mockResolvedValue([
    { urls: ["stun:stun.example:3478"], username: null, credential: null }
  ]);
  apiMocks.getMediaRelay.mockReset();
  apiMocks.getMediaRelay.mockResolvedValue({
    room_id: "DEFAULT",
    status: "active",
    mode: "media_relay",
    server_side_audio_processing: true,
    server_side_noise_cancelling: true,
    noise: defaultNoiseConfig,
    participants: users.map((user) => ({
      user_id: user.id,
      tracks: [{ track_id: "audio-main", kind: "audio" }]
    }))
  });
  apiMocks.answerServerMediaOffer.mockReset();
  apiMocks.answerServerMediaOffer.mockResolvedValue({
    room_id: "DEFAULT",
    user_id: "user_a",
    audio_track_id: "audio-main",
    sdp: "server-answer",
    state: "negotiating"
  });
  apiMocks.closeServerMediaSession.mockReset();
  apiMocks.closeServerMediaSession.mockResolvedValue({});
  apiMocks.leaveRoom.mockReset();
  apiMocks.registerMediaTrack.mockReset();
  apiMocks.registerMediaTrack.mockResolvedValue({});
  apiMocks.startMediaRelay.mockReset();
  apiMocks.startMediaRelay.mockResolvedValue({});
  apiMocks.stopMediaRelay.mockReset();
  apiMocks.updateMediaRelaySettings.mockReset();
  apiMocks.updateMediaRelaySettings.mockResolvedValue({});
  apiMocks.updateMediaRelaySubscriptions.mockReset();
  apiMocks.updateMediaRelaySubscriptions.mockResolvedValue({});
  getUserMedia.mockResolvedValue({
    getAudioTracks: () => [localAudioTrack]
  });
  vi.stubGlobal("WebSocket", MockWebSocket);
  vi.stubGlobal("RTCPeerConnection", MockPeerConnection);
  vi.stubGlobal("MediaStream", MockMediaStream);
  vi.stubGlobal("AudioContext", MockAudioContext);
  const createElement = document.createElement.bind(document);
  vi.spyOn(document, "createElement").mockImplementation((tagName: string) => {
    if (tagName === "audio") {
      return {
        autoplay: false,
        hidden: false,
        setAttribute: vi.fn(),
        play: playAudio,
        remove: removeAudio,
        srcObject: null
      } as unknown as HTMLAudioElement;
    }
    return createElement(tagName);
  });
  vi.spyOn(document.body, "append").mockImplementation(vi.fn());
  Object.defineProperty(navigator, "mediaDevices", {
    configurable: true,
    value: {
      getUserMedia
    }
  });
});
