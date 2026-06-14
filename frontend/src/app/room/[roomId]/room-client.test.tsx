import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { RoomClient } from "./room-client";

const send = vi.fn();
const sockets: MockWebSocket[] = [];
const setRemoteDescription = vi.fn();
const setLocalDescription = vi.fn();
const addIceCandidate = vi.fn();
const createOffer = vi.fn(async () => ({ type: "offer", sdp: "local-offer" }));
const createAnswer = vi.fn(async () => ({ type: "answer", sdp: "local-answer" }));

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
  addTrack = vi.fn();
  addIceCandidate = addIceCandidate;
  createAnswer = createAnswer;
  createOffer = createOffer;
  setLocalDescription = setLocalDescription;
  setRemoteDescription = setRemoteDescription;
}

vi.mock("@/lib/api", async () => {
  const actual = await vi.importActual<typeof import("@/lib/api")>("@/lib/api");
  return {
    ...actual,
    joinRoom: vi.fn(async () => ({
      user: {
        id: "user_a",
        nickname: "Ada",
        joined_at: new Date().toISOString(),
        noise: { provider: "off", intensity: 0.5, voice_activity_threshold: 0.35 }
      },
      room: { room_id: "DEFAULT", users: [] }
    })),
    leaveRoom: vi.fn(),
    shareRoomUrl: () => "http://localhost:3000/room/DEFAULT"
  };
});

describe("RoomClient", () => {
  beforeEach(() => {
    sockets.length = 0;
    sessionStorage.clear();
    send.mockClear();
    setRemoteDescription.mockClear();
    setLocalDescription.mockClear();
    addIceCandidate.mockClear();
    createOffer.mockClear();
    createAnswer.mockClear();
    vi.stubGlobal("WebSocket", MockWebSocket);
    vi.stubGlobal("RTCPeerConnection", MockPeerConnection);
    Object.defineProperty(navigator, "mediaDevices", {
      configurable: true,
      value: {
        getUserMedia: vi.fn(async () => ({
          getAudioTracks: () => [{ id: "track" }]
        }))
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

    expect(setRemoteDescription).not.toHaveBeenCalled();
    expect(send).not.toHaveBeenCalled();
  });

  it("answers incoming offers after audio is started", async () => {
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());
    fireEvent.click(screen.getByText("Connect audio"));
    await waitFor(() => expect(navigator.mediaDevices.getUserMedia).toHaveBeenCalled());
    send.mockClear();

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

    await waitFor(() => expect(setRemoteDescription).toHaveBeenCalledWith({ type: "offer", sdp: "remote-offer" }));
    expect(send).toHaveBeenCalledWith(
      JSON.stringify({
        type: "answer",
        room_id: "DEFAULT",
        sender_id: "user_a",
        recipient_id: "user_b",
        payload: { type: "answer", sdp: "local-answer" }
      })
    );
  });
});
