import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { NoiseCancellationConfig, UserProfile } from "@/lib/api";
import { RoomClient } from "./room-client";

const send = vi.fn();
const sockets: MockWebSocket[] = [];
const getUserMedia = vi.fn();
const stopTrack = vi.fn();
const addRemoteTrack = vi.fn();
const removeAudio = vi.fn();
const playAudio = vi.fn();
const stopMediaRelay = vi.fn();
const apiMocks = vi.hoisted(() => ({
  addServerMediaIceCandidate: vi.fn(),
  answerServerMediaOffer: vi.fn(),
  getIceServers: vi.fn(async () => [{ urls: ["stun:stun.example:3478"], username: null, credential: null }]),
  getServerMediaIceCandidates: vi.fn(),
  leaveRoom: vi.fn(),
  registerMediaTrack: vi.fn(),
  startMediaRelay: vi.fn(),
  stopMediaRelay: vi.fn()
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
    noise: { provider: "off", intensity: 0.5, voice_activity_threshold: 0.35 }
  };
}

const users = [makeUser("user_a", "Ada"), makeUser("user_b", "Bob"), makeUser("user_c", "Cam")];

function sentMessages() {
  return send.mock.calls.map(([message]) => JSON.parse(message as string));
}

function selectPeerMesh() {
  fireEvent.change(screen.getByLabelText("Audio mode"), { target: { value: "peer_mesh" } });
}

class MockWebSocket {
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
      user: users[0],
      room: { room_id: "DEFAULT", users }
    })),
    getIceServers: apiMocks.getIceServers,
    leaveRoom: apiMocks.leaveRoom,
    startMediaRelay: apiMocks.startMediaRelay,
    stopMediaRelay: apiMocks.stopMediaRelay,
    registerMediaTrack: apiMocks.registerMediaTrack,
    answerServerMediaOffer: apiMocks.answerServerMediaOffer,
    addServerMediaIceCandidate: apiMocks.addServerMediaIceCandidate,
    getServerMediaIceCandidates: apiMocks.getServerMediaIceCandidates,
    shareRoomUrl: () => "http://localhost:3000/room/DEFAULT"
  };
});

describe("RoomClient", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  beforeEach(() => {
    sockets.length = 0;
    peerConnections.length = 0;
    sessionStorage.clear();
    send.mockClear();
    getUserMedia.mockReset();
    stopTrack.mockClear();
    addRemoteTrack.mockClear();
    removeAudio.mockClear();
    playAudio.mockReset();
    playAudio.mockResolvedValue(undefined);
    stopMediaRelay.mockClear();
    createOfferMock.mockReset();
    createOfferMock.mockImplementation(async (peer: MockPeerConnection) => ({
      type: "offer",
      sdp: `local-offer-${peerConnections.indexOf(peer)}`
    }));
    apiMocks.getIceServers.mockClear();
    apiMocks.getIceServers.mockResolvedValue([
      { urls: ["stun:stun.example:3478"], username: null, credential: null }
    ]);
    apiMocks.addServerMediaIceCandidate.mockReset();
    apiMocks.addServerMediaIceCandidate.mockResolvedValue({
      room_id: "DEFAULT",
      user_id: "user_a",
      candidate: "candidate:local"
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
    apiMocks.leaveRoom.mockReset();
    apiMocks.registerMediaTrack.mockReset();
    apiMocks.registerMediaTrack.mockResolvedValue({});
    apiMocks.startMediaRelay.mockReset();
    apiMocks.startMediaRelay.mockResolvedValue({});
    apiMocks.stopMediaRelay.mockReset();
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

  it("opens presence websocket without requesting microphone", async () => {
    render(<RoomClient roomId="DEFAULT" />);

    await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());
    expect(navigator.mediaDevices.getUserMedia).not.toHaveBeenCalled();
    expect(send).not.toHaveBeenCalled();
  });

  it("ignores webrtc signals before audio is started", async () => {
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());

    sockets[0].onmessage?.(
      new MessageEvent("message", {
        data: JSON.stringify({
          type: "offer",
          room_id: "DEFAULT",
          sender_id: "user_b",
          payload: { type: "offer", sdp: "remote-offer" }
        })
      })
    );

    expect(peerConnections).toHaveLength(0);
    expect(send).not.toHaveBeenCalled();
  });

  it("defaults to server relay mode and negotiates server media without mesh signalling", async () => {
    const noise: NoiseCancellationConfig = {
      provider: "rnnoise",
      intensity: 0.8,
      voice_activity_threshold: 0.2
    };
    localStorage.setItem("lyre.noise", JSON.stringify(noise));
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());

    expect(screen.getByLabelText("Audio mode")).toHaveValue("server_relay");
    fireEvent.click(screen.getByText("Connect audio"));

    await waitFor(() => expect(navigator.mediaDevices.getUserMedia).toHaveBeenCalled());
    expect(apiMocks.getIceServers.mock.invocationCallOrder[0]).toBeLessThan(
      getUserMedia.mock.invocationCallOrder[0]
    );
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());
    expect(apiMocks.startMediaRelay).toHaveBeenCalledWith("DEFAULT", noise);
    expect(apiMocks.registerMediaTrack).toHaveBeenCalledWith("DEFAULT", "user_a", "audio-main", "audio");
    expect(peerConnections).toHaveLength(1);
    expect(peerConnections[0].setLocalDescription).toHaveBeenCalledWith({ type: "offer", sdp: "local-offer-0" });
    expect(peerConnections[0].setRemoteDescription).toHaveBeenCalledWith({ type: "answer", sdp: "server-answer" });
    expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledWith("DEFAULT", "user_a", "audio-main", "local-offer-0");
    expect(send).not.toHaveBeenCalled();
    expect(screen.getByText("Server relay audio connected")).toBeInTheDocument();
    expect(screen.getByLabelText("Audio mode")).toBeDisabled();
    expect(screen.getByText("Connect audio")).toBeDisabled();
  });

  it("keeps server relay playback setup local to remote tracks", async () => {
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());
    fireEvent.click(screen.getByText("Connect audio"));
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

    act(() => {
      peerConnections[0].ontrack?.({
        track: { id: "remote-track" },
        streams: []
      } as unknown as RTCTrackEvent);
    });

    expect(addRemoteTrack).toHaveBeenCalledWith({ id: "remote-track" });
    expect(playAudio).toHaveBeenCalledOnce();
  });

  it("cleans server relay local media on leave without stopping the room relay", async () => {
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());
    fireEvent.click(screen.getByText("Connect audio"));
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

    fireEvent.click(screen.getByText("Leave"));

    await waitFor(() => expect(apiMocks.leaveRoom).toHaveBeenCalledWith("DEFAULT", "user_a"));
    expect(apiMocks.stopMediaRelay).not.toHaveBeenCalled();
    expect(peerConnections[0].close).toHaveBeenCalledOnce();
    expect(stopTrack).toHaveBeenCalledOnce();
    expect(removeAudio).toHaveBeenCalledOnce();
  });

  it("keeps server relay unmount cleanup local without room mutations", async () => {
    const rendered = render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());
    fireEvent.click(screen.getByText("Connect audio"));
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

    rendered.unmount();

    expect(apiMocks.leaveRoom).not.toHaveBeenCalled();
    expect(apiMocks.stopMediaRelay).not.toHaveBeenCalled();
    expect(peerConnections[0].close).toHaveBeenCalledOnce();
    expect(stopTrack).toHaveBeenCalledOnce();
    expect(removeAudio).toHaveBeenCalledOnce();
    expect(sockets[0].close).toHaveBeenCalledOnce();
  });

  it("keeps server relay startup errors visible without stopping the room relay", async () => {
    apiMocks.registerMediaTrack.mockRejectedValueOnce(new Error("track registration failed"));
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());

    fireEvent.click(screen.getByText("Connect audio"));

    await waitFor(() => expect(screen.getByText("track registration failed")).toBeInTheDocument());
    expect(apiMocks.stopMediaRelay).not.toHaveBeenCalled();
    expect(stopTrack).toHaveBeenCalledOnce();
  });

  it("keeps initial server candidate exchange errors visible", async () => {
    apiMocks.getServerMediaIceCandidates.mockRejectedValueOnce(new Error("candidate fetch failed"));
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());

    fireEvent.click(screen.getByText("Connect audio"));

    await waitFor(() => expect(screen.getByText("candidate fetch failed")).toBeInTheDocument());
    expect(screen.queryByText("Server relay audio connected")).not.toBeInTheDocument();
    expect(apiMocks.stopMediaRelay).not.toHaveBeenCalled();
    expect(peerConnections[0].close).toHaveBeenCalledOnce();
    expect(stopTrack).toHaveBeenCalledOnce();
  });

  it("starts one peer connection per remote user and sends targeted offers", async () => {
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());
    selectPeerMesh();

    fireEvent.click(screen.getByText("Connect audio"));

    await waitFor(() => expect(navigator.mediaDevices.getUserMedia).toHaveBeenCalled());
    expect(apiMocks.getIceServers.mock.invocationCallOrder[0]).toBeLessThan(
      getUserMedia.mock.invocationCallOrder[0]
    );
    await waitFor(() => expect(peerConnections).toHaveLength(2));
    expect(sentMessages()).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ type: "offer", recipient_id: "user_b" }),
        expect.objectContaining({ type: "offer", recipient_id: "user_c" })
      ])
    );
  });

  it("does not create a second mesh session when connect audio is clicked twice", async () => {
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());
    selectPeerMesh();

    fireEvent.click(screen.getByText("Connect audio"));
    fireEvent.click(screen.getByText("Connect audio"));

    await waitFor(() => expect(peerConnections).toHaveLength(2));
    expect(navigator.mediaDevices.getUserMedia).toHaveBeenCalledOnce();
    expect(stopTrack).not.toHaveBeenCalled();
  });

  it("keeps peer-specific startup errors visible", async () => {
    createOfferMock.mockRejectedValue(new Error("offer failed"));
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());
    selectPeerMesh();

    fireEvent.click(screen.getByText("Connect audio"));

    await waitFor(() => expect(screen.getByText("offer failed")).toBeInTheDocument());
    expect(screen.queryByText("Audio offers sent")).not.toBeInTheDocument();
  });

  it("answers incoming offers after audio is started", async () => {
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());
    selectPeerMesh();
    fireEvent.click(screen.getByText("Connect audio"));
    await waitFor(() => expect(peerConnections).toHaveLength(2));
    send.mockClear();

    act(() => {
      sockets[0].onmessage?.(
        new MessageEvent("message", {
          data: JSON.stringify({
            type: "offer",
            room_id: "DEFAULT",
            sender_id: "user_d",
            recipient_id: "user_a",
            payload: { type: "offer", sdp: "remote-offer" }
          })
        })
      );
    });

    await waitFor(() => expect(peerConnections).toHaveLength(3));
    expect(peerConnections[2].setRemoteDescription).toHaveBeenCalledWith({ type: "offer", sdp: "remote-offer" });
    expect(send).toHaveBeenCalledWith(
      JSON.stringify({
        type: "answer",
        room_id: "DEFAULT",
        sender_id: "user_a",
        recipient_id: "user_d",
        payload: { type: "answer", sdp: "local-answer-undefined" }
      })
    );
  });

  it("routes incoming ice candidates to the sender peer", async () => {
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());
    selectPeerMesh();
    fireEvent.click(screen.getByText("Connect audio"));
    await waitFor(() => expect(peerConnections).toHaveLength(2));

    act(() => {
      sockets[0].onmessage?.(
        new MessageEvent("message", {
          data: JSON.stringify({
            type: "ice-candidate",
            room_id: "DEFAULT",
            sender_id: "user_c",
            recipient_id: "user_a",
            payload: { type: "ice-candidate", candidate: "candidate-c", sdp_mid: "0", sdp_m_line_index: 0 }
          })
        })
      );
    });

    await waitFor(() =>
      expect(peerConnections[1].addIceCandidate).toHaveBeenCalledWith({
        candidate: "candidate-c",
        sdpMid: "0",
        sdpMLineIndex: 0
      })
    );
    expect(peerConnections[0].addIceCandidate).not.toHaveBeenCalled();
  });

  it("offers to a newly joined user after audio has started", async () => {
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());
    selectPeerMesh();
    fireEvent.click(screen.getByText("Connect audio"));
    await waitFor(() => expect(peerConnections).toHaveLength(2));
    send.mockClear();

    act(() => {
      sockets[0].onmessage?.(
        new MessageEvent("message", {
          data: JSON.stringify({
            type: "user-joined",
            room_id: "DEFAULT",
            sender_id: "user_d",
            payload: { type: "user-joined", user: makeUser("user_d", "Dee") }
          })
        })
      );
    });

    await waitFor(() => expect(peerConnections).toHaveLength(3));
    expect(sentMessages()).toEqual(
      expect.arrayContaining([expect.objectContaining({ type: "offer", recipient_id: "user_d" })])
    );
  });

  it("closes a leaving user's peer connection", async () => {
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());
    selectPeerMesh();
    fireEvent.click(screen.getByText("Connect audio"));
    await waitFor(() => expect(peerConnections).toHaveLength(2));

    act(() => {
      sockets[0].onmessage?.(
        new MessageEvent("message", {
          data: JSON.stringify({
            type: "user-left",
            room_id: "DEFAULT",
            sender_id: "user_b",
            payload: { type: "user-left", user_id: "user_b" }
          })
        })
      );
    });

    await waitFor(() => expect(peerConnections[0].close).toHaveBeenCalledOnce());
    expect(peerConnections[1].close).not.toHaveBeenCalled();
  });

  it("closes peer connections and stops local tracks on unmount", async () => {
    const rendered = render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());
    selectPeerMesh();
    fireEvent.click(screen.getByText("Connect audio"));
    await waitFor(() => expect(peerConnections).toHaveLength(2));

    rendered.unmount();

    expect(peerConnections[0].close).toHaveBeenCalledOnce();
    expect(peerConnections[1].close).toHaveBeenCalledOnce();
    expect(stopTrack).toHaveBeenCalledOnce();
    expect(sockets[0].close).toHaveBeenCalledOnce();
  });

  it("does not start media when ice server fetch fails", async () => {
    apiMocks.getIceServers.mockRejectedValueOnce(new Error("ice unavailable"));
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());

    fireEvent.click(screen.getByText("Connect audio"));

    await waitFor(() => expect(screen.getByText("ice unavailable")).toBeInTheDocument());
    expect(navigator.mediaDevices.getUserMedia).not.toHaveBeenCalled();
    expect(peerConnections).toHaveLength(0);
  });
});
