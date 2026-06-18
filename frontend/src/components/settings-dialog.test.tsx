import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { readNickname, readNoiseConfig } from "@/lib/storage";
import { readSettingsSnapshot, resetSettingsStoreForTests } from "@/lib/settings-store";
import messages from "../../messages/en-US.json";
import { SettingsDialog } from "./settings-dialog";

const navigation = vi.hoisted(() => ({
  refresh: vi.fn()
}));

vi.mock("next/navigation", () => ({
  useRouter: () => navigation
}));

describe("SettingsDialog", () => {
  beforeEach(() => {
    localStorage.clear();
    resetSettingsStoreForTests();
    navigation.refresh.mockClear();
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
    renderWithIntl(<SettingsDialog open onOpenChange={onOpenChange} />);

    expect(screen.getByRole("tab", { name: /profile/i })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /noise/i })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /devices/i })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /advanced/i })).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("Nickname"), { target: { value: "Ada" } });
    openSettingsTab("Noise");
    await chooseSelectOption("Server Noise Cancelling", "DPDFNet");
    await chooseSelectOption("DPDFNet model", "dpdfnet8_48khz_hr");
    fireEvent.change(screen.getByLabelText("Intensity"), { target: { value: "0.75" } });
    fireEvent.change(screen.getByLabelText("Voice activity threshold"), { target: { value: "0.2" } });
    openSettingsTab("Devices");
    await chooseSelectOption("Microphone", "Studio Mic");
    await chooseSelectOption("Speaker", "Desk Speakers");
    openSettingsTab("Advanced");
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
    expect(navigation.refresh).toHaveBeenCalled();
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
    renderWithIntl(<SettingsDialog open onOpenChange={onOpenChange} />);

    openSettingsTab("Devices");
    await chooseSelectOption("Microphone", "Default microphone");
    await chooseSelectOption("Speaker", "Default speaker");
    openSettingsTab("Profile");
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
    renderWithIntl(<SettingsDialog open onOpenChange={onOpenChange} />);

    openSettingsTab("Devices");
    await chooseSelectOption("Microphone", "Default microphone");
    await chooseSelectOption("Speaker", "Default speaker");
    openSettingsTab("Profile");
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
    renderWithIntl(<SettingsDialog open onOpenChange={vi.fn()} onSave={onSave} />);

    openSettingsTab("Noise");
    await chooseSelectOption("Server Noise Cancelling", "RNNoise");
    fireEvent.click(screen.getByText("Save"));

    await waitFor(() => expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
      noise: expect.objectContaining({ provider: "rnnoise" })
    })));
  });

  it("keeps the dialog open when clicking inside it to dismiss an open dropdown", async () => {
    const onOpenChange = vi.fn();
    renderWithIntl(<SettingsDialog open onOpenChange={onOpenChange} />);

    openSettingsTab("Devices");
    fireEvent.click(screen.getByLabelText("Microphone"));
    expect(await screen.findByRole("option", { name: "Studio Mic" })).toBeInTheDocument();
    const dialog = screen.getByRole("dialog", { hidden: true });
    expect(dialog).not.toContainElement(screen.getByRole("listbox"));
    expect(within(dialog).getByText("Settings")).toBeInTheDocument();
    fireEvent.pointerDown(dialog);

    expect(onOpenChange).not.toHaveBeenCalledWith(false);
  });
});

function renderWithIntl(ui: React.ReactElement) {
  return render(
    <NextIntlClientProvider locale="en-US" messages={messages}>
      {ui}
    </NextIntlClientProvider>
  );
}

async function chooseSelectOption(label: string, option: string): Promise<void> {
  fireEvent.click(screen.getByLabelText(label));
  fireEvent.click(await screen.findByRole("option", { name: option }));
}

function openSettingsTab(name: string): void {
  fireEvent.mouseDown(screen.getByRole("tab", { name }), { button: 0, ctrlKey: false });
}
