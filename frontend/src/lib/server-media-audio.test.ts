import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ServerMediaAudioSession } from "./server-media-audio";

const apiMocks = vi.hoisted(() => ({
  addServerMediaIceCandidate: vi.fn(),
  answerServerMediaOffer: vi.fn(),
  getServerMediaIceCandidates: vi.fn()
}));

vi.mock("./api", async () => {
  const actual = await vi.importActual<typeof import("./api")>("./api");
  return {
    ...actual,
    addServerMediaIceCandidate: apiMocks.addServerMediaIceCandidate,
    answerServerMediaOffer: apiMocks.answerServerMediaOffer,
    getServerMediaIceCandidates: apiMocks.getServerMediaIceCandidates
  };
});

const stopTrack = vi.fn();
const addRemoteTrack = vi.fn();
const play = vi.fn();
const removeAudio = vi.fn();
const append = vi.fn();
const peerConnections: MockPeerConnection[] = [];

const stream = {
  getAudioTracks: () => [{ id: "local-audio", stop: stopTrack }]
} as unknown as MediaStream;

class MockMediaStream {
  addTrack = addRemoteTrack;
}

class MockPeerConnection {
  onicecandidate: ((event: RTCPeerConnectionIceEvent) => void) | null = null;
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
    iceServers: [{ urls: ["stun:stun.example:3478"], username: null, credential: null }],
    stream,
    pollIntervalMs: 10
  });
}

describe("ServerMediaAudioSession", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    peerConnections.length = 0;
    stopTrack.mockClear();
    addRemoteTrack.mockClear();
    play.mockReset();
    play.mockResolvedValue(undefined);
    removeAudio.mockClear();
    append.mockClear();
    apiMocks.addServerMediaIceCandidate.mockReset();
    apiMocks.addServerMediaIceCandidate.mockResolvedValue({
      room_id: "DEFAULT",
      user_id: "user_a",
      candidate: "local-candidate"
    });
    apiMocks.answerServerMediaOffer.mockReset();
    apiMocks.answerServerMediaOffer.mockResolvedValue({
      room_id: "DEFAULT",
      user_id: "user_a",
      audio_track_id: "audio-main",
      sdp: "server-answer",
      state: "negotiating"
    });
    apiMocks.getServerMediaIceCandidates.mockReset();
    apiMocks.getServerMediaIceCandidates.mockResolvedValue([]);
    vi.stubGlobal("RTCPeerConnection", MockPeerConnection);
    vi.stubGlobal("MediaStream", MockMediaStream);
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

  it("negotiates an offer, applies the answer, fetches candidates, and starts polling", async () => {
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
    expect(apiMocks.getServerMediaIceCandidates).toHaveBeenCalledWith("DEFAULT", "user_a", "token_a");
    expect(play).toHaveBeenCalledOnce();

    await vi.advanceTimersByTimeAsync(10);

    expect(apiMocks.getServerMediaIceCandidates).toHaveBeenCalledTimes(2);
  });

  it("posts local ICE candidates to the server-media endpoint", async () => {
    const session = makeSession();
    await session.start();

    peerConnections[0].onicecandidate?.(iceCandidateEvent());

    await vi.waitFor(() =>
      expect(apiMocks.addServerMediaIceCandidate).toHaveBeenCalledWith("DEFAULT", {
        user_id: "user_a",
        candidate: "candidate:local",
        sdp_mid: "0",
        sdp_mline_index: 0,
        username_fragment: "ufrag"
      }, "token_a")
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

    expect(apiMocks.addServerMediaIceCandidate).not.toHaveBeenCalled();

    resolveAnswer!({
      room_id: "DEFAULT",
      user_id: "user_a",
      audio_track_id: "audio-main",
      sdp: "server-answer",
      state: "negotiating"
    });
    await start;

    await vi.waitFor(() =>
      expect(apiMocks.addServerMediaIceCandidate).toHaveBeenCalledWith("DEFAULT", {
        user_id: "user_a",
        candidate: "candidate:early",
        sdp_mid: "0",
        sdp_mline_index: 0,
        username_fragment: "ufrag"
      }, "token_a")
    );
  });

  it("attaches remote tracks to the playback stream and starts audio playback", async () => {
    const session = makeSession();
    await session.start();

    peerConnections[0].ontrack?.({
      track: { id: "remote-track" },
      streams: []
    } as unknown as RTCTrackEvent);

    expect(addRemoteTrack).toHaveBeenCalledWith({ id: "remote-track" });
    expect(play).toHaveBeenCalledTimes(2);
  });

  it("deduplicates repeated server ICE candidates", async () => {
    apiMocks.getServerMediaIceCandidates.mockResolvedValue([
      {
        room_id: "DEFAULT",
        user_id: "user_a",
        candidate: "candidate:server",
        sdp_mid: "0",
        sdp_mline_index: 0,
        username_fragment: null
      }
    ]);
    const session = makeSession();

    await session.start();
    await vi.advanceTimersByTimeAsync(10);

    expect(peerConnections[0].addIceCandidate).toHaveBeenCalledTimes(1);
  });

  it("closes peer resources, stops local tracks, and removes playback audio", async () => {
    const session = makeSession();
    await session.start();

    session.close();
    await vi.advanceTimersByTimeAsync(10);

    expect(peerConnections[0].close).toHaveBeenCalledOnce();
    expect(stopTrack).toHaveBeenCalledOnce();
    expect(removeAudio).toHaveBeenCalledOnce();
    expect(apiMocks.getServerMediaIceCandidates).toHaveBeenCalledOnce();
  });
});
