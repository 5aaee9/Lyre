import { beforeEach, describe, expect, it, vi } from "vitest";
import type { IceServerConfig, UserProfile } from "./api";
import { MeshAudioSession } from "./mesh-audio";
import type { SignalMessage } from "./signalling";

const makeUser = (id: string): UserProfile => ({
  id,
  nickname: id,
  joined_at: "2026-06-15T00:00:00Z",
  noise: { provider: "off", intensity: 0.5, voice_activity_threshold: 0.35 }
});

describe("MeshAudioSession", () => {
  const iceServers: IceServerConfig[] = [{ urls: ["stun:stun.example:3478"], username: null, credential: null }];
  const send = vi.fn();
  const stop = vi.fn();
  const stream = {
    getAudioTracks: () => [{ id: "track", stop }]
  } as unknown as MediaStream;
  const peerInstances: MockPeerConnection[] = [];

  class MockPeerConnection {
    onicecandidate: ((event: RTCPeerConnectionIceEvent) => void) | null = null;
    addTrack = vi.fn();
    addIceCandidate = vi.fn();
    close = vi.fn();
    createAnswer = vi.fn(async () => ({ type: "answer", sdp: `answer-${peerInstances.indexOf(this)}` }));
    createOffer = vi.fn(async () => ({ type: "offer", sdp: `offer-${peerInstances.indexOf(this)}` }));
    setLocalDescription = vi.fn();
    setRemoteDescription = vi.fn();

    constructor() {
      peerInstances.push(this);
    }
  }

  function session() {
    return new MeshAudioSession({
      roomId: "DEFAULT",
      currentUserId: "user_a",
      iceServers,
      stream,
      send
    });
  }

  beforeEach(() => {
    send.mockClear();
    stop.mockClear();
    peerInstances.length = 0;
    vi.stubGlobal("RTCPeerConnection", MockPeerConnection);
  });

  it("connects to each remote user with targeted offers", async () => {
    const audio = session();
    await audio.connectToUsers([makeUser("user_a"), makeUser("user_b"), makeUser("user_c")]);

    expect(peerInstances).toHaveLength(2);
    expect(send).toHaveBeenCalledWith(expect.objectContaining({ type: "offer", recipient_id: "user_b" }));
    expect(send).toHaveBeenCalledWith(expect.objectContaining({ type: "offer", recipient_id: "user_c" }));
  });

  it("answers incoming offers on the sender peer", async () => {
    const audio = session();
    await audio.handleSignal({
      type: "offer",
      room_id: "DEFAULT",
      sender_id: "user_b",
      recipient_id: "user_a",
      payload: { type: "offer", sdp: "remote-offer" }
    });

    expect(peerInstances).toHaveLength(1);
    expect(peerInstances[0].setRemoteDescription).toHaveBeenCalledWith({ type: "offer", sdp: "remote-offer" });
    expect(send).toHaveBeenCalledWith(expect.objectContaining({ type: "answer", recipient_id: "user_b" }));
  });

  it("applies answers and ice candidates to the sender peer only", async () => {
    const audio = session();
    await audio.connectToUsers([makeUser("user_b"), makeUser("user_c")]);
    await audio.handleSignal({
      type: "answer",
      room_id: "DEFAULT",
      sender_id: "user_c",
      recipient_id: "user_a",
      payload: { type: "answer", sdp: "answer-c" }
    });
    await audio.handleSignal({
      type: "ice-candidate",
      room_id: "DEFAULT",
      sender_id: "user_c",
      recipient_id: "user_a",
      payload: { type: "ice-candidate", candidate: "candidate-c", sdp_mid: "0", sdp_m_line_index: 0 }
    });

    expect(peerInstances[0].setRemoteDescription).not.toHaveBeenCalledWith({ type: "answer", sdp: "answer-c" });
    expect(peerInstances[1].setRemoteDescription).toHaveBeenCalledWith({ type: "answer", sdp: "answer-c" });
    expect(peerInstances[1].addIceCandidate).toHaveBeenCalledWith({
      candidate: "candidate-c",
      sdpMid: "0",
      sdpMLineIndex: 0
    });
  });

  it("ignores targeted media signals for a different recipient", async () => {
    const audio = session();
    await audio.handleSignal({
      type: "offer",
      room_id: "DEFAULT",
      sender_id: "user_b",
      recipient_id: "other_user",
      payload: { type: "offer", sdp: "remote-offer" }
    } as SignalMessage);

    expect(peerInstances).toHaveLength(0);
    expect(send).not.toHaveBeenCalled();
  });

  it("ignores stale answers and ice candidates without an existing sender peer", async () => {
    const audio = session();
    await audio.handleSignal({
      type: "answer",
      room_id: "DEFAULT",
      sender_id: "user_b",
      recipient_id: "user_a",
      payload: { type: "answer", sdp: "stale-answer" }
    });
    await audio.handleSignal({
      type: "ice-candidate",
      room_id: "DEFAULT",
      sender_id: "user_b",
      recipient_id: "user_a",
      payload: { type: "ice-candidate", candidate: "stale-candidate" }
    });

    expect(peerInstances).toHaveLength(0);
  });

  it("closes removed peers and stops local tracks on close", async () => {
    const audio = session();
    await audio.connectToUsers([makeUser("user_b"), makeUser("user_c")]);

    audio.removePeer("user_b");
    expect(peerInstances[0].close).toHaveBeenCalledOnce();
    expect(peerInstances[1].close).not.toHaveBeenCalled();

    audio.close();
    expect(peerInstances[1].close).toHaveBeenCalledOnce();
    expect(stop).toHaveBeenCalledOnce();
  });
});
