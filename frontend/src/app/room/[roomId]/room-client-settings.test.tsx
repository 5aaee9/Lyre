import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { defaultNoiseConfig, useSettingsStore } from "@/lib/settings-store";
import { apiMocks, getUserMedia, peerConnections, stopTrack } from "./room-client-test-utils";
import { RoomClient } from "./room-client";

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
    await chooseSelectOption("Server Noise Cancelling", "RNNoise");
    fireEvent.change(screen.getByLabelText("Intensity"), { target: { value: "0.75" } });
    fireEvent.click(screen.getByLabelText("Browser noise suppression"));
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
});

async function chooseSelectOption(label: string, option: string): Promise<void> {
  fireEvent.click(screen.getByLabelText(label));
  fireEvent.click(await screen.findByRole("option", { name: option }));
}
