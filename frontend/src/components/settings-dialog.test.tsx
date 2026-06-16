import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { readNickname, readNoiseConfig } from "@/lib/storage";
import { readSettingsSnapshot, resetSettingsStoreForTests } from "@/lib/settings-store";
import { SettingsDialog } from "./settings-dialog";

describe("SettingsDialog", () => {
  beforeEach(() => {
    localStorage.clear();
    resetSettingsStoreForTests();
  });

  it("saves settings and closes the dialog", async () => {
    const onOpenChange = vi.fn();
    render(<SettingsDialog open onOpenChange={onOpenChange} />);

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
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
  });

  it("calls onSave with the saved settings snapshot", async () => {
    const onSave = vi.fn();
    render(<SettingsDialog open onOpenChange={vi.fn()} onSave={onSave} />);

    fireEvent.change(screen.getByLabelText("Server Noise Cancelling"), { target: { value: "rnnoise" } });
    fireEvent.click(screen.getByText("Save"));

    await waitFor(() => expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
      noise: expect.objectContaining({ provider: "rnnoise" })
    })));
  });
});
