import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { parseServerMediaSourceTrackId, ServerMediaAudioSession } from "./server-media-audio";

const apiMocks = vi.hoisted(() => ({
  answerServerMediaOffer: vi.fn()
}));

vi.mock("./api", async () => {
  const actual = await vi.importActual<typeof import("./api")>("./api");
  return {
    ...actual,
    answerServerMediaOffer: apiMocks.answerServerMediaOffer
  };
});

const stopTrack = vi.fn();
const play = vi.fn();
const removeAudio = vi.fn();
const append = vi.fn();
const socketSend = vi.fn();
const peerConnections: MockPeerConnection[] = [];
const audioContexts: MockAudioContext[] = [];
const mediaStreamSources: MockAudioSource[] = [];
const gainNodes: MockGainNode[] = [];

const localAudioTrack = { id: "local-audio", enabled: true, stop: stopTrack };

const stream = {
  getAudioTracks: () => [localAudioTrack]
} as unknown as MediaStream;

class MockMediaStream {
  tracks: unknown[] = [];
  addTrack = vi.fn((track: unknown) => {
    this.tracks.push(track);
  });
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

  constructor(readonly stream: MediaStream) {
    mediaStreamSources.push(this);
  }
}

class MockAudioContext {
  state = "suspended";
  destination = {};
  createMediaStreamSource = vi.fn((stream: MediaStream) => new MockAudioSource(stream));
  createGain = vi.fn(() => new MockGainNode());
  close = vi.fn();
  resume = vi.fn(async () => {
    this.state = "running";
  });

  constructor() {
    audioContexts.push(this);
  }
}

class MockPeerConnection {
  iceConnectionState: RTCIceConnectionState = "new";
  onicecandidate: ((event: RTCPeerConnectionIceEvent) => void) | null = null;
  oniceconnectionstatechange: (() => void) | null = null;
  ontrack: ((event: RTCTrackEvent) => void) | null = null;
  addIceCandidate = vi.fn();
  addTrack = vi.fn();
  close = vi.fn();
  createOffer = vi.fn(async () => ({ type: "offer", sdp: "local-offer" }));
  setLocalDescription = vi.fn();
  setRemoteDescription = vi.fn();

  constructor() {
    peerConnections.push(this);
  }
}

class MockWebSocket {
  static readonly OPEN = 1;
  static readonly CLOSED = 3;
}

function iceCandidateEvent(candidate = "candidate:local"): RTCPeerConnectionIceEvent {
  return {
    candidate: {
      toJSON: () => ({
        candidate,
        sdpMid: "0",
        sdpMLineIndex: 0,
        usernameFragment: "ufrag"
      })
    }
  } as RTCPeerConnectionIceEvent;
}

function makeSession() {
  return new ServerMediaAudioSession({
    roomId: "DEFAULT",
    userId: "user_a",
    accessToken: "token_a",
    socket: { readyState: WebSocket.OPEN, send: socketSend } as unknown as WebSocket,
    iceServers: [{ urls: ["stun:stun.example:3478"], username: null, credential: null }],
    stream,
    pollIntervalMs: 10
  });
}

describe("ServerMediaAudioSession", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    peerConnections.length = 0;
    audioContexts.length = 0;
    mediaStreamSources.length = 0;
    gainNodes.length = 0;
    localAudioTrack.enabled = true;
    stopTrack.mockClear();
    play.mockReset();
    play.mockResolvedValue(undefined);
    removeAudio.mockClear();
    append.mockClear();
    socketSend.mockClear();
    apiMocks.answerServerMediaOffer.mockReset();
    apiMocks.answerServerMediaOffer.mockResolvedValue({
      room_id: "DEFAULT",
      user_id: "user_a",
      audio_track_id: "audio-main",
      sdp: "server-answer",
      state: "negotiating"
    });
    vi.stubGlobal("WebSocket", MockWebSocket);
    vi.stubGlobal("RTCPeerConnection", MockPeerConnection);
    vi.stubGlobal("MediaStream", MockMediaStream);
    vi.stubGlobal("AudioContext", MockAudioContext);
    vi.spyOn(document, "createElement").mockImplementation((tagName: string) => {
      if (tagName === "audio") {
        return {
          autoplay: false,
          hidden: false,
          setAttribute: vi.fn(),
          play,
          remove: removeAudio,
          srcObject: null
        } as unknown as HTMLAudioElement;
      }
      return document.createElement(tagName);
    });
    vi.spyOn(document.body, "append").mockImplementation(append);
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it("negotiates an offer, applies the answer, requests candidates, and starts polling", async () => {
    const session = makeSession();

    await session.start();

    expect(peerConnections[0].createOffer).toHaveBeenCalledOnce();
    expect(peerConnections[0].setLocalDescription).toHaveBeenCalledWith({
      type: "offer",
      sdp: "local-offer"
    });
    expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledWith(
      "DEFAULT",
      "user_a",
      "audio-main",
      "local-offer",
      "token_a"
    );
    expect(peerConnections[0].setRemoteDescription).toHaveBeenCalledWith({
      type: "answer",
      sdp: "server-answer"
    });
    expect(socketSend).toHaveBeenCalledWith(JSON.stringify({
      type: "server-media-ice-candidates-request",
      room_id: "DEFAULT",
      sender_id: "user_a",
      recipient_id: "user_a",
      payload: { type: "server-media-ice-candidates-request" }
    }));
    await vi.advanceTimersByTimeAsync(10);

    expect(socketSend).toHaveBeenCalledTimes(2);
  });

  it("sends local ICE candidates over the room websocket", async () => {
    const session = makeSession();
    await session.start();

    peerConnections[0].onicecandidate?.(iceCandidateEvent());

    await vi.waitFor(() =>
      expect(socketSend).toHaveBeenCalledWith(JSON.stringify({
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
      }))
    );
  });

  it("queues local ICE candidates until the server media offer exists", async () => {
    let resolveAnswer: (value: unknown) => void;
    apiMocks.answerServerMediaOffer.mockReturnValueOnce(
      new Promise((resolve) => {
        resolveAnswer = resolve;
      })
    );
    const session = makeSession();
    const start = session.start();

    await vi.waitFor(() => expect(peerConnections[0].setLocalDescription).toHaveBeenCalledOnce());
    peerConnections[0].onicecandidate?.(iceCandidateEvent("candidate:early"));
    await Promise.resolve();

    expect(socketSend).not.toHaveBeenCalledWith(expect.stringContaining("candidate:early"));

    resolveAnswer!({
      room_id: "DEFAULT",
      user_id: "user_a",
      audio_track_id: "audio-main",
      sdp: "server-answer",
      state: "negotiating"
    });
    await start;

    await vi.waitFor(() =>
      expect(socketSend).toHaveBeenCalledWith(JSON.stringify({
        type: "server-media-ice-candidate",
        room_id: "DEFAULT",
        sender_id: "user_a",
        recipient_id: "user_a",
        payload: {
          type: "server-media-ice-candidate",
          candidate: "candidate:early",
          sdp_mid: "0",
          sdp_mline_index: 0,
          username_fragment: "ufrag"
        }
      }))
    );
  });

  it("parses server-media source track ids", () => {
    expect(parseServerMediaSourceTrackId("lyre-user:user_b:audio")).toBe("user_b");
    expect(parseServerMediaSourceTrackId("lyre-user:alice%40example.com:audio")).toBe("alice@example.com");
    expect(parseServerMediaSourceTrackId("remote-track")).toBeNull();
    expect(parseServerMediaSourceTrackId("lyre-user:%E0%A4%A:audio")).toBeNull();
  });

  it("connects valid source-user tracks through per-user Web Audio gain", async () => {
    const session = makeSession();
    await session.start();

    peerConnections[0].ontrack?.({
      track: { id: "lyre-user:user_b:audio" },
      streams: []
    } as unknown as RTCTrackEvent);

    expect(audioContexts).toHaveLength(1);
    expect(mediaStreamSources[0].connect).toHaveBeenCalledWith(gainNodes[0]);
    expect(gainNodes[0].connect).toHaveBeenCalledWith(audioContexts[0].destination);

    session.setUserAudioSettings("user_b", { muted: false, volumePercent: 150 });
    expect(gainNodes[0].gain.value).toBe(1.5);

    session.setUserAudioSettings("user_b", { muted: true, volumePercent: 150 });
    expect(gainNodes[0].gain.value).toBe(0);
  });

  it("resumes the playback audio context when a source track starts", async () => {
    const session = makeSession();
    await session.start();

    peerConnections[0].ontrack?.({
      track: { id: "lyre-user:user_b:audio" },
      streams: []
    } as unknown as RTCTrackEvent);

    expect(audioContexts[0].resume).toHaveBeenCalledOnce();
  });

  it("clamps applied per-user gain at the audio boundary", async () => {
    const session = makeSession();
    await session.start();

    peerConnections[0].ontrack?.({
      track: { id: "lyre-user:user_b:audio" },
      streams: []
    } as unknown as RTCTrackEvent);

    session.setUserAudioSettings("user_b", { muted: false, volumePercent: -10 });
    expect(gainNodes[0].gain.value).toBe(0);

    session.setUserAudioSettings("user_b", { muted: false, volumePercent: 175 });
    expect(gainNodes[0].gain.value).toBe(1.5);
  });

  it("applies initial per-user settings when a source track starts", async () => {
    const session = new ServerMediaAudioSession({
      roomId: "DEFAULT",
      userId: "user_a",
      accessToken: "token_a",
      socket: { readyState: WebSocket.OPEN, send: socketSend } as unknown as WebSocket,
      iceServers: [{ urls: ["stun:stun.example:3478"], username: null, credential: null }],
      stream,
      pollIntervalMs: 10,
      userAudio: {
        user_b: { muted: false, volumePercent: 125 },
        user_c: { muted: true, volumePercent: 150 }
      }
    });
    await session.start();

    peerConnections[0].ontrack?.({
      track: { id: "lyre-user:user_b:audio" },
      streams: []
    } as unknown as RTCTrackEvent);
    peerConnections[0].ontrack?.({
      track: { id: "lyre-user:user_c:audio" },
      streams: []
    } as unknown as RTCTrackEvent);

    expect(gainNodes[0].gain.value).toBe(1.25);
    expect(gainNodes[1].gain.value).toBe(0);
  });

  it("reports invalid source track ids without playing them", async () => {
    const onError = vi.fn();
    const session = new ServerMediaAudioSession({
      roomId: "DEFAULT",
      userId: "user_a",
      accessToken: "token_a",
      socket: { readyState: WebSocket.OPEN, send: socketSend } as unknown as WebSocket,
      iceServers: [{ urls: ["stun:stun.example:3478"], username: null, credential: null }],
      stream,
      pollIntervalMs: 10,
      onError
    });
    await session.start();

    peerConnections[0].ontrack?.({
      track: { id: "remote-track" },
      streams: []
    } as unknown as RTCTrackEvent);

    expect(audioContexts).toHaveLength(0);
    expect(onError).toHaveBeenCalledWith("Ignored server media track with invalid id: remote-track");
  });

  it("toggles local microphone tracks without closing the session", async () => {
    const session = makeSession();
    await session.start();

    session.setMuted(true);

    expect(localAudioTrack.enabled).toBe(false);
    expect(peerConnections[0].close).not.toHaveBeenCalled();
    expect(stopTrack).not.toHaveBeenCalled();

    session.setMuted(false);

    expect(localAudioTrack.enabled).toBe(true);
  });

  it("reports ICE disconnection and failure through the interruption callback", async () => {
    const onConnectionInterrupted = vi.fn();
    const session = new ServerMediaAudioSession({
      roomId: "DEFAULT",
      userId: "user_a",
      accessToken: "token_a",
      socket: { readyState: WebSocket.OPEN, send: socketSend } as unknown as WebSocket,
      iceServers: [{ urls: ["stun:stun.example:3478"], username: null, credential: null }],
      stream,
      pollIntervalMs: 10,
      onConnectionInterrupted
    });
    await session.start();

    peerConnections[0].iceConnectionState = "connected";
    peerConnections[0].oniceconnectionstatechange?.();
    peerConnections[0].iceConnectionState = "disconnected";
    peerConnections[0].oniceconnectionstatechange?.();
    peerConnections[0].iceConnectionState = "failed";
    peerConnections[0].oniceconnectionstatechange?.();

    expect(onConnectionInterrupted).toHaveBeenCalledTimes(2);
  });

  it("deduplicates repeated server ICE candidates", async () => {
    const session = makeSession();

    await session.start();
    await session.handleSignal({
      type: "server-media-ice-candidates",
      room_id: "DEFAULT",
      sender_id: "user_a",
      recipient_id: "user_a",
      payload: {
        type: "server-media-ice-candidates",
        candidates: [
          {
            room_id: "DEFAULT",
            user_id: "user_a",
            candidate: "candidate:server",
            sdp_mid: "0",
            sdp_mline_index: 0,
            username_fragment: null
          },
          {
            room_id: "DEFAULT",
            user_id: "user_a",
            candidate: "candidate:server",
            sdp_mid: "0",
            sdp_mline_index: 0,
            username_fragment: null
          }
        ]
      }
    });

    expect(peerConnections[0].addIceCandidate).toHaveBeenCalledTimes(1);
  });

  it("reports websocket connectivity failures when starting candidate requests", async () => {
    const session = new ServerMediaAudioSession({
      roomId: "DEFAULT",
      userId: "user_a",
      accessToken: "token_a",
      socket: { readyState: WebSocket.CLOSED, send: socketSend } as unknown as WebSocket,
      iceServers: [{ urls: ["stun:stun.example:3478"], username: null, credential: null }],
      stream,
      pollIntervalMs: 10,
      onError: vi.fn()
    });

    await expect(session.start()).rejects.toThrow("Audio signalling websocket is not connected");
  });

  it("closes peer resources, stops local tracks, and disconnects playback nodes", async () => {
    const session = makeSession();
    await session.start();
    peerConnections[0].ontrack?.({
      track: { id: "lyre-user:user_b:audio" },
      streams: []
    } as unknown as RTCTrackEvent);

    session.close();
    await vi.advanceTimersByTimeAsync(10);

    expect(peerConnections[0].close).toHaveBeenCalledOnce();
    expect(stopTrack).toHaveBeenCalledOnce();
    expect(mediaStreamSources[0].disconnect).toHaveBeenCalledOnce();
    expect(gainNodes[0].disconnect).toHaveBeenCalledOnce();
    expect(audioContexts[0].close).toHaveBeenCalledOnce();
    expect(socketSend).toHaveBeenCalledTimes(1);
  });
});
