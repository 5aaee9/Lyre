import { afterEach, beforeEach, vi } from "vitest";
import type { UserProfile } from "@/lib/api";
import { defaultNoiseConfig, resetSettingsStoreForTests } from "@/lib/settings-store";

const send = vi.fn();
const sockets: MockWebSocket[] = [];
const getUserMedia = vi.fn();
const stopTrack = vi.fn();
const addRemoteTrack = vi.fn();
const removeAudio = vi.fn();
const playAudio = vi.fn();
const apiMocks = vi.hoisted(() => ({
  answerServerMediaOffer: vi.fn(),
  closeServerMediaSession: vi.fn(),
  getIceServers: vi.fn(async () => [{ urls: ["stun:stun.example:3478"], username: null, credential: null }]),
  leaveRoom: vi.fn(),
  registerMediaTrack: vi.fn(),
  startMediaRelay: vi.fn(),
  stopMediaRelay: vi.fn(),
  updateMediaRelaySettings: vi.fn()
}));
const peerConnections: MockPeerConnection[] = [];
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
  onicecandidate: ((event: RTCPeerConnectionIceEvent) => void) | null = null;
  ontrack: ((event: RTCTrackEvent) => void) | null = null;
  addTrack = vi.fn();
  addIceCandidate = vi.fn();
  close = vi.fn();
  createAnswer = vi.fn(async () => ({ type: "answer", sdp: `local-answer-${this.remoteUserId}` }));
  createOffer = vi.fn(async () => createOfferMock(this));
  setLocalDescription = vi.fn();
  setRemoteDescription = vi.fn();
  remoteUserId?: string;

  constructor() {
    peerConnections.push(this);
  }
}

class MockMediaStream {
  addTrack = addRemoteTrack;
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
    getIceServers: apiMocks.getIceServers,
    leaveRoom: apiMocks.leaveRoom,
    startMediaRelay: apiMocks.startMediaRelay,
    stopMediaRelay: apiMocks.stopMediaRelay,
    registerMediaTrack: apiMocks.registerMediaTrack,
    answerServerMediaOffer: apiMocks.answerServerMediaOffer,
    closeServerMediaSession: apiMocks.closeServerMediaSession,
    updateMediaRelaySettings: apiMocks.updateMediaRelaySettings,
    shareRoomUrl: () => "http://localhost:3000/room/DEFAULT"
  };
});

export {
  addRemoteTrack,
  apiMocks,
  getUserMedia,
  makeUser,
  peerConnections,
  playAudio,
  removeAudio,
  send,
  sockets,
  stopTrack
};

afterEach(() => {
  vi.restoreAllMocks();
});

beforeEach(() => {
  sockets.length = 0;
  peerConnections.length = 0;
  localStorage.clear();
  sessionStorage.clear();
  resetSettingsStoreForTests();
  send.mockClear();
  getUserMedia.mockReset();
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
  getUserMedia.mockResolvedValue({
    getAudioTracks: () => [{ id: "track", stop: stopTrack }]
  });
  vi.stubGlobal("WebSocket", MockWebSocket);
  vi.stubGlobal("RTCPeerConnection", MockPeerConnection);
  vi.stubGlobal("MediaStream", MockMediaStream);
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
