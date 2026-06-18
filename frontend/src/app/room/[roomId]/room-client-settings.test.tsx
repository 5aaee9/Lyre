import { fireEvent, render as testingLibraryRender, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { describe, expect, it } from "vitest";
import { defaultNoiseConfig, useSettingsStore } from "@/lib/settings-store";
import messages from "../../../../messages/en-US.json";
import { addRemoteTrack, apiMocks, getUserMedia, peerConnections, playAudio, stopTrack } from "./room-client-test-utils";
import { RoomClient } from "./room-client";

function render(ui: React.ReactElement) {
  return testingLibraryRender(
    <NextIntlClientProvider locale="en-US" messages={messages}>
      {ui}
    </NextIntlClientProvider>
  );
}

describe("RoomClient settings", () => {
  it("opens settings as a dialog in the room", async () => {
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());

    fireEvent.click(screen.getByText("Settings"));

    expect(screen.getByRole("dialog", { name: "Settings" })).toBeInTheDocument();
  });

  it("updates server noise settings and recreates server media after saving settings while audio is connected", async () => {
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

    fireEvent.click(screen.getByText("Settings"));
    openSettingsTab("Noise");
    await chooseSelectOption("Server Noise Cancelling", "RNNoise");
    fireEvent.change(screen.getByLabelText("Intensity"), { target: { value: "0.75" } });
    fireEvent.click(screen.getByText("Save"));

    await waitFor(() =>
      expect(apiMocks.updateMediaRelaySettings).toHaveBeenCalledWith(
        "DEFAULT",
        "user_a",
        expect.objectContaining({
          provider: "rnnoise",
          intensity: 0.75
        }),
        "token_a"
      )
    );
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledTimes(2));
    expect(peerConnections[0].close).toHaveBeenCalledOnce();
    expect(stopTrack).toHaveBeenCalledOnce();
    expect(getUserMedia).toHaveBeenLastCalledWith({
      audio: {
        echoCancellation: true,
        autoGainControl: true,
        noiseSuppression: true
      }
    });
    expect(screen.queryByRole("dialog", { name: "Settings" })).not.toBeInTheDocument();
  });

  it("keeps remote playback alive after switching server noise from off to RNNoise", async () => {
    useSettingsStore.getState().setNoise({
      ...defaultNoiseConfig,
      provider: "off"
    });
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

    fireEvent.click(screen.getByText("Settings"));
    openSettingsTab("Noise");
    await chooseSelectOption("Server Noise Cancelling", "RNNoise");
    fireEvent.click(screen.getByText("Save"));
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledTimes(2));

    peerConnections[1].ontrack?.({
      track: { id: "lyre-user:user_b:audio" },
      streams: []
    } as unknown as RTCTrackEvent);

    expect(addRemoteTrack).toHaveBeenCalledWith(expect.objectContaining({ id: "lyre-user:user_b:audio" }));
    expect(playAudio).toHaveBeenCalledOnce();
  });

  it("can turn server noise cancelling off while audio is connected", async () => {
    useSettingsStore.getState().setNoise({
      provider: "rnnoise",
      intensity: 0.8,
      voice_activity_threshold: 0.2,
      dpdfnet: defaultNoiseConfig.dpdfnet
    });
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

    fireEvent.click(screen.getByText("Settings"));
    openSettingsTab("Noise");
    await chooseSelectOption("Server Noise Cancelling", "Off");
    fireEvent.click(screen.getByText("Save"));

    await waitFor(() =>
      expect(apiMocks.updateMediaRelaySettings).toHaveBeenCalledWith(
        "DEFAULT",
        "user_a",
        expect.objectContaining({ provider: "off" }),
        "token_a"
      )
    );
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledTimes(2));
  });

  it("saves device selections without recreating the active audio session", async () => {
    Object.defineProperty(navigator, "mediaDevices", {
      configurable: true,
      value: {
        getUserMedia,
        enumerateDevices: async () => [
          { kind: "audioinput", deviceId: "mic-a", label: "Studio Mic" },
          { kind: "audiooutput", deviceId: "speaker-a", label: "Desk Speakers" }
        ]
      }
    });
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

    fireEvent.click(screen.getByText("Settings"));
    openSettingsTab("Devices");
    await chooseSelectOption("Microphone", "Studio Mic");
    await chooseSelectOption("Speaker", "Desk Speakers");
    fireEvent.click(screen.getByText("Save"));

    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Settings" })).not.toBeInTheDocument());
    expect(apiMocks.updateMediaRelaySettings).not.toHaveBeenCalled();
    expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce();
    expect(peerConnections[0].close).not.toHaveBeenCalled();
    expect(stopTrack).not.toHaveBeenCalled();
    expect(getUserMedia).toHaveBeenCalledTimes(1);
    expect(useSettingsStore.getState().audioDevices).toEqual({
      inputDeviceId: "mic-a",
      outputDeviceId: "speaker-a"
    });
  });

  it("saves device selections without recreating audio after a prior noise update", async () => {
    Object.defineProperty(navigator, "mediaDevices", {
      configurable: true,
      value: {
        getUserMedia,
        enumerateDevices: async () => [
          { kind: "audioinput", deviceId: "mic-a", label: "Studio Mic" },
          { kind: "audiooutput", deviceId: "speaker-a", label: "Desk Speakers" }
        ]
      }
    });
    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

    fireEvent.click(screen.getByText("Settings"));
    openSettingsTab("Noise");
    await chooseSelectOption("Server Noise Cancelling", "RNNoise");
    fireEvent.click(screen.getByText("Save"));
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledTimes(2));

    const closeCallsAfterNoiseUpdate = peerConnections.reduce((count, peer) => count + peer.close.mock.calls.length, 0);
    const stoppedTracksAfterNoiseUpdate = stopTrack.mock.calls.length;
    fireEvent.click(screen.getByText("Settings"));
    openSettingsTab("Devices");
    await chooseSelectOption("Microphone", "Studio Mic");
    await chooseSelectOption("Speaker", "Desk Speakers");
    fireEvent.click(screen.getByText("Save"));

    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Settings" })).not.toBeInTheDocument());
    expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledTimes(2);
    expect(getUserMedia).toHaveBeenCalledTimes(2);
    expect(peerConnections.reduce((count, peer) => count + peer.close.mock.calls.length, 0)).toBe(closeCallsAfterNoiseUpdate);
    expect(stopTrack).toHaveBeenCalledTimes(stoppedTracksAfterNoiseUpdate);
  });
});

async function chooseSelectOption(label: string, option: string): Promise<void> {
  fireEvent.click(screen.getByLabelText(label));
  fireEvent.click(await screen.findByRole("option", { name: option }));
}

function openSettingsTab(name: string): void {
  fireEvent.mouseDown(screen.getByRole("tab", { name }), { button: 0, ctrlKey: false });
}
