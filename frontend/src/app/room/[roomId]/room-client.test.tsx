import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { NoiseCancellationConfig } from "@/lib/api";
import { defaultNoiseConfig, useSettingsStore } from "@/lib/settings-store";
import {
  addRemoteTrack,
  apiMocks,
  getUserMedia,
  localAudioTrack,
  makeUser,
  peerConnections,
  playAudio,
  removeAudio,
  send,
  sockets,
  stopTrack
} from "./room-client-test-utils";
import { RoomClient } from "./room-client";

describe("RoomClient", () => {
  it("waits for the room websocket to open before starting automatic audio", async () => {
    render(<RoomClient roomId="DEFAULT" />);

    expect(apiMocks.startMediaRelay).not.toHaveBeenCalled();

    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());
  });

  it("does not show peer noise cancelling providers in the room user list", async () => {
    render(<RoomClient roomId="DEFAULT" />);

    await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());

    expect(screen.getByText("Ada")).toBeInTheDocument();
    expect(screen.queryByText("off")).not.toBeInTheDocument();
    expect(screen.queryByText("rnnoise")).not.toBeInTheDocument();
    expect(screen.queryByText("deepfilternet")).not.toBeInTheDocument();
  });

  it("rejoins when stored room session belongs to another room", async () => {
    sessionStorage.setItem(
      "lyre.roomSession",
      JSON.stringify({ roomId: "OTHER", accessToken: "old_token", user: makeUser("old_user") })
    );

    render(<RoomClient roomId="DEFAULT" />);

    await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());
    expect(sessionStorage.getItem("lyre.roomSession")).toContain("token_a");
  });

  it("rejoins when stored room session is malformed", async () => {
    sessionStorage.setItem(
      "lyre.roomSession",
      JSON.stringify({ roomId: "DEFAULT", accessToken: "old_token", user: {} })
    );

    render(<RoomClient roomId="DEFAULT" />);

    await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());
    expect(sessionStorage.getItem("lyre.roomSession")).toContain("token_a");
  });

  it("does not route peer mesh offers through server relay audio", async () => {
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

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

    expect(peerConnections).toHaveLength(1);
    expect(peerConnections[0].setRemoteDescription).toHaveBeenCalledWith({ type: "answer", sdp: "server-answer" });
  });

  it("starts server relay audio automatically after joining", async () => {
    render(<RoomClient roomId="DEFAULT" />);

    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

    expect(screen.queryByText("Connect audio")).not.toBeInTheDocument();
    expect(screen.getByText("Mute")).toBeInTheDocument();
    expect(navigator.mediaDevices.getUserMedia).toHaveBeenCalledOnce();
    expect(apiMocks.startMediaRelay).toHaveBeenCalledOnce();
    expect(apiMocks.registerMediaTrack).toHaveBeenCalledOnce();
  });

  it("toggles local microphone mute without recreating audio", async () => {
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

    fireEvent.click(screen.getByText("Mute"));

    expect(localAudioTrack.enabled).toBe(false);
    expect(screen.getByText("Unmute")).toBeInTheDocument();
    expect(peerConnections).toHaveLength(1);

    fireEvent.click(screen.getByText("Unmute"));

    expect(localAudioTrack.enabled).toBe(true);
    expect(screen.getByText("Mute")).toBeInTheDocument();
    expect(apiMocks.startMediaRelay).toHaveBeenCalledOnce();
  });

  it("defaults to server relay mode and negotiates server media without mesh signalling", async () => {
    const noise: NoiseCancellationConfig = {
      provider: "rnnoise",
      intensity: 0.8,
      voice_activity_threshold: 0.2,
      dpdfnet: defaultNoiseConfig.dpdfnet
    };
    useSettingsStore.getState().setNoise(noise);
    render(<RoomClient roomId="DEFAULT" />);

    expect(screen.queryByLabelText("Audio mode")).not.toBeInTheDocument();
    await waitFor(() => expect(navigator.mediaDevices.getUserMedia).toHaveBeenCalled());
    expect(apiMocks.getIceServers.mock.invocationCallOrder[0]).toBeLessThan(
      getUserMedia.mock.invocationCallOrder[0]
    );
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());
    expect(apiMocks.startMediaRelay).toHaveBeenCalledWith("DEFAULT", noise, "token_a");
    expect(apiMocks.registerMediaTrack).toHaveBeenCalledWith("DEFAULT", "user_a", "audio-main", "audio", "token_a");
    expect(peerConnections).toHaveLength(1);
    expect(peerConnections[0].setLocalDescription).toHaveBeenCalledWith({ type: "offer", sdp: "local-offer-0" });
    expect(peerConnections[0].setRemoteDescription).toHaveBeenCalledWith({ type: "answer", sdp: "server-answer" });
    expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledWith(
      "DEFAULT",
      "user_a",
      "audio-main",
      "local-offer-0",
      "token_a"
    );
    expect(send).toHaveBeenCalledWith(JSON.stringify({
      type: "server-media-ice-candidates-request",
      room_id: "DEFAULT",
      sender_id: "user_a",
      recipient_id: "user_a",
      payload: { type: "server-media-ice-candidates-request" }
    }));
    expect(screen.getByText("Server relay audio connected")).toBeInTheDocument();
    expect(screen.queryByText("Connect audio")).not.toBeInTheDocument();
  });

  it("dispatches incoming server media candidates to the active audio session", async () => {
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

    act(() => {
      sockets[0].onmessage?.(
        new MessageEvent("message", {
          data: JSON.stringify({
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
                }
              ]
            }
          })
        })
      );
    });

    await waitFor(() =>
      expect(peerConnections[0].addIceCandidate).toHaveBeenCalledWith({
        candidate: "candidate:server",
        sdpMid: "0",
        sdpMLineIndex: 0,
        usernameFragment: undefined
      })
    );
  });

  it("keeps server relay playback setup local to remote tracks", async () => {
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

    act(() => {
      peerConnections[0].ontrack?.({
        track: { id: "remote-track" },
        streams: []
      } as unknown as RTCTrackEvent);
    });

    expect(addRemoteTrack).toHaveBeenCalledWith({ id: "remote-track" });
    expect(playAudio).toHaveBeenCalledTimes(2);
  });

  it("cleans server relay local and server media on leave without stopping the room relay", async () => {
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

    fireEvent.click(screen.getByText("Leave"));

    await waitFor(() => expect(apiMocks.closeServerMediaSession).toHaveBeenCalledWith("DEFAULT", "user_a", "token_a"));
    await waitFor(() => expect(apiMocks.leaveRoom).toHaveBeenCalledWith("DEFAULT", "user_a", "token_a"));
    expect(apiMocks.closeServerMediaSession.mock.invocationCallOrder[0]).toBeLessThan(
      apiMocks.leaveRoom.mock.invocationCallOrder[0]
    );
    expect(apiMocks.leaveRoom.mock.invocationCallOrder[0]).toBeLessThan(
      sockets[0].close.mock.invocationCallOrder[0]
    );
    expect(sessionStorage.getItem("lyre.roomSession")).toBeNull();
    expect(apiMocks.stopMediaRelay).not.toHaveBeenCalled();
    expect(peerConnections[0].close).toHaveBeenCalledOnce();
    expect(stopTrack).toHaveBeenCalledOnce();
    expect(removeAudio).toHaveBeenCalledOnce();
  });

  it("keeps server relay unmount cleanup local without room mutations", async () => {
    sessionStorage.setItem(
      "lyre.roomSession",
      JSON.stringify({ roomId: "DEFAULT", accessToken: "token_a", user: makeUser("user_a") })
    );
    const rendered = render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

    rendered.unmount();

    expect(apiMocks.leaveRoom).not.toHaveBeenCalled();
    expect(apiMocks.closeServerMediaSession).not.toHaveBeenCalled();
    expect(apiMocks.stopMediaRelay).not.toHaveBeenCalled();
    expect(peerConnections[0].close).toHaveBeenCalledOnce();
    expect(stopTrack).toHaveBeenCalledOnce();
    expect(removeAudio).toHaveBeenCalledOnce();
    expect(sockets[0].close).toHaveBeenCalledOnce();
    expect(sessionStorage.getItem("lyre.roomSession")).toBeNull();
  });

  it("clears stored room session when websocket closes", async () => {
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());
    expect(sessionStorage.getItem("lyre.roomSession")).toContain("token_a");

    act(() => {
      sockets[0].onclose?.();
    });

    expect(sessionStorage.getItem("lyre.roomSession")).toBeNull();
    expect(screen.getByText("Disconnected")).toBeInTheDocument();
  });

  it("reconnects local audio when ICE is interrupted without restarting relay registration", async () => {
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

    act(() => {
      peerConnections[0].iceConnectionState = "disconnected";
      peerConnections[0].oniceconnectionstatechange?.();
    });

    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledTimes(2));

    expect(peerConnections).toHaveLength(2);
    expect(peerConnections[0].close).toHaveBeenCalledOnce();
    expect(apiMocks.startMediaRelay).toHaveBeenCalledOnce();
    expect(apiMocks.registerMediaTrack).toHaveBeenCalledOnce();
    expect(screen.getByText("Server relay audio connected")).toBeInTheDocument();
  });

  it("unblocks later reconnect attempts when one reconnect fails", async () => {
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());
    apiMocks.getIceServers.mockRejectedValueOnce(new Error("temporary ice failure"));

    act(() => {
      peerConnections[0].iceConnectionState = "failed";
      peerConnections[0].oniceconnectionstatechange?.();
    });
    await waitFor(() => expect(screen.getByText("temporary ice failure")).toBeInTheDocument());

    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledTimes(2), { timeout: 2_000 });
    expect(apiMocks.startMediaRelay).toHaveBeenCalledOnce();
    expect(apiMocks.registerMediaTrack).toHaveBeenCalledOnce();
  });

  it("cleans server relay startup failures after relay start without stopping the room relay", async () => {
    apiMocks.registerMediaTrack.mockRejectedValueOnce(new Error("track registration failed"));
    render(<RoomClient roomId="DEFAULT" />);

    await waitFor(() => expect(screen.getByText("track registration failed")).toBeInTheDocument());
    expect(apiMocks.closeServerMediaSession).toHaveBeenCalledWith("DEFAULT", "user_a", "token_a");
    expect(apiMocks.stopMediaRelay).not.toHaveBeenCalled();
    expect(stopTrack).toHaveBeenCalledOnce();
  });

  it("keeps original server relay startup error visible when cleanup fails", async () => {
    apiMocks.registerMediaTrack.mockRejectedValueOnce(new Error("track registration failed"));
    apiMocks.closeServerMediaSession.mockRejectedValueOnce(new Error("cleanup failed"));
    render(<RoomClient roomId="DEFAULT" />);

    await waitFor(() => expect(screen.getByText("track registration failed")).toBeInTheDocument());
    expect(screen.queryByText("cleanup failed")).not.toBeInTheDocument();
    expect(apiMocks.closeServerMediaSession).toHaveBeenCalledWith("DEFAULT", "user_a", "token_a");
    expect(apiMocks.stopMediaRelay).not.toHaveBeenCalled();
  });

  it("keeps missing signalling websocket startup errors visible", async () => {
    apiMocks.registerMediaTrack.mockImplementationOnce(async () => {
      sockets[0].readyState = WebSocket.CLOSED;
    });
    render(<RoomClient roomId="DEFAULT" />);

    await waitFor(() => expect(screen.getByText("Audio signalling websocket is not connected")).toBeInTheDocument());
    expect(screen.queryByText("Server relay audio connected")).not.toBeInTheDocument();
    expect(apiMocks.stopMediaRelay).not.toHaveBeenCalled();
    expect(apiMocks.closeServerMediaSession).toHaveBeenCalledWith("DEFAULT", "user_a", "token_a");
    expect(stopTrack).toHaveBeenCalledOnce();
    expect(peerConnections).toHaveLength(0);
  });

  it("does not start media when ice server fetch fails", async () => {
    apiMocks.getIceServers.mockRejectedValueOnce(new Error("ice unavailable"));
    render(<RoomClient roomId="DEFAULT" />);

    await waitFor(() => expect(screen.getByText("ice unavailable")).toBeInTheDocument());
    expect(navigator.mediaDevices.getUserMedia).not.toHaveBeenCalled();
    expect(peerConnections).toHaveLength(0);
    expect(apiMocks.startMediaRelay).not.toHaveBeenCalled();
  });
});
