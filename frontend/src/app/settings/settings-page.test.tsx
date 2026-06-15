import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import SettingsPage from "./page";
import { readNickname, readNoiseConfig } from "@/lib/storage";
import { readSettingsSnapshot, resetSettingsStoreForTests } from "@/lib/settings-store";

describe("SettingsPage", () => {
  beforeEach(() => {
    localStorage.clear();
    resetSettingsStoreForTests();
  });

  it("saves provider, numeric noise parameters, and audio processing", () => {
    render(<SettingsPage />);

    fireEvent.change(screen.getByLabelText("Nickname"), { target: { value: "Ada" } });
    fireEvent.change(screen.getByLabelText("Server Noise Cancelling"), { target: { value: "dpdfnet" } });
    fireEvent.change(screen.getByLabelText("DPDFNet model"), { target: { value: "dpdfnet8_48khz_hr" } });
    fireEvent.change(screen.getByLabelText("Intensity"), { target: { value: "0.75" } });
    fireEvent.change(screen.getByLabelText("Voice activity threshold"), { target: { value: "0.2" } });
    fireEvent.click(screen.getByLabelText("Echo cancellation"));
    fireEvent.click(screen.getByText("Save"));

    expect(readNickname()).toBe("Ada");
    expect(readNoiseConfig()).toEqual({
      provider: "dpdfnet",
      intensity: 0.75,
      voice_activity_threshold: 0.2,
      dpdfnet: {
        model: "dpdfnet8_48khz_hr"
      }
    });
    expect(readSettingsSnapshot().audioProcessing).toEqual({
      echoCancellation: false,
      autoGainControl: true,
      noiseSuppression: false
    });
  });
});
