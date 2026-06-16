import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { NoiseCancellationConfig } from "@/lib/api";
import { defaultNoiseConfig, useSettingsStore } from "@/lib/settings-store";
import {
  apiMocks,
  audioContexts,
  gainNodes,
  getUserMedia,
  localAudioTrack,
  makeUser,
  peerConnections,
  peerStatsReports,
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
    expect(apiMocks.updateMediaRelaySubscriptions).toHaveBeenCalledWith(
      "DEFAULT",
      "user_a",
      ["user_b", "user_c"],
      "token_a"
    );
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

  it("renders playback controls for remote users only", async () => {
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());

    expect(screen.queryByText("Mute Ada")).not.toBeInTheDocument();
    expect(screen.getByText("Mute Bob")).toBeInTheDocument();
    expect(screen.getByLabelText("Bob volume")).toHaveValue("100");
    expect(screen.getByText("Mute Cam")).toBeInTheDocument();
    expect(screen.getByLabelText("Cam volume")).toHaveValue("100");
  });

  it("hides audio diagnostics until enabled in settings", async () => {
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

    expect(screen.queryByText("Audio diagnostics")).not.toBeInTheDocument();

    fireEvent.click(screen.getByLabelText("Settings"));
    fireEvent.click(screen.getByLabelText("Audio diagnostics"));
    fireEvent.click(screen.getByText("Save"));

    await waitFor(() => expect(screen.getByText("Audio diagnostics")).toBeInTheDocument());
  });

  it("shows WebRTC audio counters when diagnostics are enabled", async () => {
    useSettingsStore.getState().setAudioDiagnosticsEnabled(true);
    peerStatsReports[0] = new Map([
      ["outbound-audio", {
        type: "outbound-rtp",
        kind: "audio",
        packetsSent: 12,
        bytesSent: 3456
      }],
      ["inbound-audio", {
        type: "inbound-rtp",
        kind: "audio",
        packetsReceived: 7,
        bytesReceived: 890,
        packetsLost: 1
      }],
      ["remote-inbound-audio", {
        type: "remote-inbound-rtp",
        kind: "audio",
        packetsLost: 2,
        roundTripTime: 0.031
      }]
    ]);
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());
    peerConnections[0].getReceivers.mockReturnValue([
      { track: { id: "lyre-user:user_b:audio" } as MediaStreamTrack } as RTCRtpReceiver,
      { track: { id: "receiver-only-track" } as MediaStreamTrack } as RTCRtpReceiver
    ]);
    act(() => {
      peerConnections[0].ontrack?.({
        track: { id: "lyre-user:user_b:audio" },
        streams: []
      } as unknown as RTCTrackEvent);
    });

    await waitFor(() => expect(screen.getByText("12")).toBeInTheDocument());

    expect(screen.getByText("3456")).toBeInTheDocument();
    expect(screen.getByText("7")).toBeInTheDocument();
    expect(screen.getByText("890")).toBeInTheDocument();
    expect(screen.getByText("31 ms")).toBeInTheDocument();
    expect(screen.getAllByText("user_b, user_c")).toHaveLength(2);
    expect(screen.getAllByText("lyre-user:user_b:audio")).toHaveLength(2);
    expect(screen.getByText("lyre-user:user_b:audio, receiver-only-track")).toBeInTheDocument();
    expect(screen.getAllByText("none").length).toBeGreaterThanOrEqual(2);
    expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce();
  });

  it("shows rejected server media track ids in diagnostics", async () => {
    useSettingsStore.getState().setAudioDiagnosticsEnabled(true);
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

    act(() => {
      peerConnections[0].ontrack?.({
        track: { id: "remote-track" },
        streams: []
      } as unknown as RTCTrackEvent);
    });

    await waitFor(() => expect(screen.getByText("Ignored server media track with invalid id: remote-track")).toBeInTheDocument());

    expect(screen.getAllByText("remote-track")).toHaveLength(2);
    expect(screen.getAllByText("Ignored server media track with invalid id: remote-track")).toHaveLength(2);
  });

  it("uses persisted muted users before first server-media connect", async () => {
    useSettingsStore.getState().setUserAudioSettings("user_b", { muted: true });

    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

    expect(apiMocks.updateMediaRelaySubscriptions).toHaveBeenCalledWith(
      "DEFAULT",
      "user_a",
      ["user_c"],
      "token_a"
    );
    expect(screen.getByText("Unmute Bob")).toBeInTheDocument();
  });

  it("subscribes only to registered relay participants after joining", async () => {
    apiMocks.getMediaRelay.mockResolvedValueOnce({
      room_id: "DEFAULT",
      status: "active",
      mode: "media_relay",
      server_side_audio_processing: true,
      server_side_noise_cancelling: true,
      noise: defaultNoiseConfig,
      participants: [{
        user_id: "user_a",
        tracks: [{ track_id: "audio-main", kind: "audio" }]
      }]
    });

    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

    expect(apiMocks.updateMediaRelaySubscriptions).toHaveBeenCalledWith(
      "DEFAULT",
      "user_a",
      [],
      "token_a"
    );
    expect(screen.queryByText("failed to update media relay subscriptions: 409")).not.toBeInTheDocument();
  });

  it("refreshes relay participants after a joined user finishes track registration", async () => {
    apiMocks.getMediaRelay
      .mockResolvedValueOnce({
        room_id: "DEFAULT",
        status: "active",
        mode: "media_relay",
        server_side_audio_processing: true,
        server_side_noise_cancelling: true,
        noise: defaultNoiseConfig,
        participants: [{
          user_id: "user_a",
          tracks: [{ track_id: "audio-main", kind: "audio" }]
        }]
      })
      .mockResolvedValue({
        room_id: "DEFAULT",
        status: "active",
        mode: "media_relay",
        server_side_audio_processing: true,
        server_side_noise_cancelling: true,
        noise: defaultNoiseConfig,
        participants: ["user_a", "user_b", "user_c"].map((userId) => ({
          user_id: userId,
          tracks: [{ track_id: "audio-main", kind: "audio" }]
        }))
      });

    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(apiMocks.updateMediaRelaySubscriptions).toHaveBeenCalledWith(
      "DEFAULT",
      "user_a",
      [],
      "token_a"
    ));

    await waitFor(() => expect(apiMocks.updateMediaRelaySubscriptions).toHaveBeenLastCalledWith(
      "DEFAULT",
      "user_a",
      ["user_b", "user_c"],
      "token_a"
    ), { timeout: 2_000 });
  });

  it("muting a remote user updates subscriptions and recreates server media without toggling microphone mute", async () => {
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

    fireEvent.click(screen.getByText("Mute Bob"));

    await waitFor(() => expect(apiMocks.updateMediaRelaySubscriptions).toHaveBeenLastCalledWith(
      "DEFAULT",
      "user_a",
      ["user_c"],
      "token_a"
    ));
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledTimes(2));
    expect(localAudioTrack.enabled).toBe(true);
    expect(peerConnections[0].close).toHaveBeenCalledOnce();
  });

  it("keeps the existing server media session when subscription update fails", async () => {
    apiMocks.updateMediaRelaySubscriptions.mockResolvedValueOnce({});
    apiMocks.updateMediaRelaySubscriptions.mockRejectedValueOnce(new Error("subscription failed"));
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

    fireEvent.click(screen.getByText("Mute Bob"));

    await waitFor(() => expect(screen.getByText("subscription failed")).toBeInTheDocument());
    expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce();
    expect(peerConnections[0].close).not.toHaveBeenCalled();
  });

  it("updates remote gain when volume control changes", async () => {
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

    fireEvent.change(screen.getByLabelText("Bob volume"), { target: { value: "125" } });

    await waitFor(() => expect(useSettingsStore.getState().userAudio.user_b.volumePercent).toBe(125));
  });

  it("unmuting recreates server media with the restored user volume snapshot", async () => {
    useSettingsStore.getState().setUserAudioSettings("user_b", {
      muted: true,
      volumePercent: 125
    });
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

    fireEvent.click(screen.getByText("Unmute Bob"));

    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledTimes(2));
    act(() => {
      peerConnections[1].ontrack?.({
        track: { id: "lyre-user:user_b:audio" },
        streams: []
      } as unknown as RTCTrackEvent);
    });

    expect(gainNodes[0].gain.value).toBe(1.25);
  });

  it("resumes remote playback from a user gesture without recreating server media", async () => {
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());
    act(() => {
      peerConnections[0].ontrack?.({
        track: { id: "lyre-user:user_b:audio" },
        streams: []
      } as unknown as RTCTrackEvent);
    });
    audioContexts[0].state = "suspended";
    audioContexts[0].resume.mockClear();

    fireEvent.click(screen.getByText("Resume audio"));

    await waitFor(() => expect(audioContexts[0].resume).toHaveBeenCalledOnce());
    expect(peerConnections).toHaveLength(1);
    expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce();
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
