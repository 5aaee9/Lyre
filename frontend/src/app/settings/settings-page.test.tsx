import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import SettingsPage from "./page";
import { readNickname, readNoiseConfig } from "@/lib/storage";

describe("SettingsPage", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("saves provider and numeric noise parameters", () => {
    render(<SettingsPage />);

    fireEvent.change(screen.getByLabelText("Nickname"), { target: { value: "Ada" } });
    fireEvent.change(screen.getByLabelText("Noise cancellation"), { target: { value: "deepfilternet" } });
    fireEvent.change(screen.getByLabelText("Intensity"), { target: { value: "0.75" } });
    fireEvent.change(screen.getByLabelText("Voice activity threshold"), { target: { value: "0.2" } });
    fireEvent.click(screen.getByText("Save"));

    expect(readNickname()).toBe("Ada");
    expect(readNoiseConfig()).toEqual({
      provider: "deepfilternet",
      intensity: 0.75,
      voice_activity_threshold: 0.2
    });
  });
});
