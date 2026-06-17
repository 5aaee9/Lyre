import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { readNickname, readNoiseConfig } from "@/lib/storage";
import { readSettingsSnapshot, resetSettingsStoreForTests } from "@/lib/settings-store";
import { SettingsDialog } from "./settings-dialog";

describe("SettingsDialog", () => {
  beforeEach(() => {
    localStorage.clear();
    resetSettingsStoreForTests();
    Object.defineProperty(navigator, "mediaDevices", {
      configurable: true,
      value: {
        enumerateDevices: vi.fn(async () => [
          { kind: "audioinput", deviceId: "mic-a", label: "Studio Mic" },
          { kind: "audiooutput", deviceId: "speaker-a", label: "Desk Speakers" }
        ])
      }
    });
  });

  it("saves settings and closes the dialog", async () => {
    const onOpenChange = vi.fn();
    render(<SettingsDialog open onOpenChange={onOpenChange} />);

    fireEvent.change(screen.getByLabelText("Nickname"), { target: { value: "Ada" } });
    await chooseSelectOption("Server Noise Cancelling", "DPDFNet");
    await chooseSelectOption("DPDFNet model", "dpdfnet8_48khz_hr");
    await chooseSelectOption("Microphone", "Studio Mic");
    await chooseSelectOption("Speaker", "Desk Speakers");
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
      noiseSuppression: true
    });
    expect(readSettingsSnapshot().audioDevices).toEqual({
      inputDeviceId: "mic-a",
      outputDeviceId: "speaker-a"
    });
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
  });

  it("keeps default devices when device enumeration fails", async () => {
    Object.defineProperty(navigator, "mediaDevices", {
      configurable: true,
      value: {
        enumerateDevices: vi.fn(async () => {
          throw new Error("blocked");
        })
      }
    });
    const onOpenChange = vi.fn();
    render(<SettingsDialog open onOpenChange={onOpenChange} />);

    fireEvent.click(screen.getByLabelText("Microphone"));
    expect(await screen.findByRole("option", { name: "Default microphone" })).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText("Speaker"));
    expect(await screen.findByRole("option", { name: "Default speaker" })).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Nickname"), { target: { value: "Ada" } });
    fireEvent.click(screen.getByText("Save"));

    expect(readSettingsSnapshot().audioDevices).toEqual({
      inputDeviceId: "",
      outputDeviceId: ""
    });
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
  });

  it("keeps default devices when media device enumeration is unavailable", async () => {
    Object.defineProperty(navigator, "mediaDevices", {
      configurable: true,
      value: undefined
    });
    const onOpenChange = vi.fn();
    render(<SettingsDialog open onOpenChange={onOpenChange} />);

    fireEvent.click(screen.getByLabelText("Microphone"));
    expect(await screen.findByRole("option", { name: "Default microphone" })).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText("Speaker"));
    expect(await screen.findByRole("option", { name: "Default speaker" })).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Nickname"), { target: { value: "Ada" } });
    fireEvent.click(screen.getByText("Save"));

    expect(readSettingsSnapshot().audioDevices).toEqual({
      inputDeviceId: "",
      outputDeviceId: ""
    });
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
  });

  it("calls onSave with the saved settings snapshot", async () => {
    const onSave = vi.fn();
    render(<SettingsDialog open onOpenChange={vi.fn()} onSave={onSave} />);

    await chooseSelectOption("Server Noise Cancelling", "RNNoise");
    fireEvent.click(screen.getByText("Save"));

    await waitFor(() => expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
      noise: expect.objectContaining({ provider: "rnnoise" })
    })));
  });
});

async function chooseSelectOption(label: string, option: string): Promise<void> {
  fireEvent.click(screen.getByLabelText(label));
  fireEvent.click(await screen.findByRole("option", { name: option }));
}
