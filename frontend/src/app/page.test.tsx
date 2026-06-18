import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import Home from "./page";
import { joinRoom } from "@/lib/api";
import { defaultNoiseConfig, resetSettingsStoreForTests, useSettingsStore } from "@/lib/settings-store";

const navigation = vi.hoisted(() => ({
  push: vi.fn()
}));

vi.mock("next/navigation", () => ({
  useRouter: () => navigation
}));

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return {
    ...actual,
    joinRoom: vi.fn()
  };
});

describe("Home", () => {
  beforeEach(() => {
    localStorage.clear();
    sessionStorage.clear();
    resetSettingsStoreForTests();
    navigation.push.mockClear();
    vi.mocked(joinRoom).mockResolvedValue({
      access_token: "token_a",
      user: {
        id: "user_a",
        nickname: "User A",
        joined_at: "2026-06-14T00:00:00Z",
        noise: {
          provider: "deepfilternet",
          intensity: 0.8,
          voice_activity_threshold: 0.15,
          dpdfnet: defaultNoiseConfig.dpdfnet
        }
      },
      room: {
        room_id: "DEFAULT",
        users: []
      }
    });
  });

  it("joins with stored noise settings without showing noise controls", async () => {
    useSettingsStore.getState().setNoise({
      provider: "deepfilternet",
      intensity: 0.8,
      voice_activity_threshold: 0.15,
      dpdfnet: defaultNoiseConfig.dpdfnet
    });

    render(<Home />);

    expect(screen.queryByText("Noise cancellation")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /join voice/i }));

    await waitFor(() => {
      expect(joinRoom).toHaveBeenCalledWith("DEFAULT", {
        nickname: "",
        noise: {
          provider: "deepfilternet",
          intensity: 0.8,
          voice_activity_threshold: 0.15,
          dpdfnet: defaultNoiseConfig.dpdfnet
        }
      });
    });
    expect(JSON.parse(sessionStorage.getItem("lyre.roomSession") ?? "{}")).toMatchObject({
      roomId: "DEFAULT",
      accessToken: "token_a",
      user: { id: "user_a" }
    });
  });

  it("opens settings before joining and uses the saved noise settings", async () => {
    render(<Home />);

    fireEvent.click(screen.getByRole("button", { name: /settings/i }));
    expect(screen.getByRole("dialog", { name: "Settings" })).toBeInTheDocument();

    openSettingsTab("Noise");
    await chooseSelectOption("Server Noise Cancelling", "RNNoise");
    fireEvent.click(screen.getByText("Save"));

    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Settings" })).not.toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: /join voice/i }));

    await waitFor(() => {
      expect(joinRoom).toHaveBeenCalledWith("DEFAULT", {
        nickname: "",
        noise: expect.objectContaining({ provider: "rnnoise" })
      });
    });
  });

  it("submits a custom room and nickname from the keyboard-first form", async () => {
    render(<Home />);

    fireEvent.change(screen.getByLabelText(/room id/i), { target: { value: "design/review" } });
    fireEvent.change(screen.getByLabelText(/nickname/i), { target: { value: "Nora" } });
    fireEvent.submit(screen.getByRole("button", { name: /join voice/i }).closest("form")!);

    await waitFor(() => {
      expect(joinRoom).toHaveBeenCalledWith("design/review", {
        nickname: "Nora",
        noise: defaultNoiseConfig
      });
    });
    expect(navigation.push).toHaveBeenCalledWith("/room/design%2Freview");
    expect(JSON.parse(sessionStorage.getItem("lyre.roomSession") ?? "{}")).toMatchObject({
      roomId: "design/review",
      accessToken: "token_a",
      user: { id: "user_a" }
    });
  });

  it("shows a recoverable error when joining fails", async () => {
    vi.mocked(joinRoom).mockRejectedValueOnce(new Error("relay unavailable"));

    render(<Home />);
    fireEvent.click(screen.getByRole("button", { name: /join voice/i }));

    expect(await screen.findByRole("alert")).toHaveTextContent("relay unavailable");
    expect(screen.getByRole("button", { name: /join voice/i })).toBeEnabled();
    expect(navigation.push).not.toHaveBeenCalled();
  });
});

async function chooseSelectOption(label: string, option: string): Promise<void> {
  fireEvent.click(screen.getByLabelText(label));
  fireEvent.click(await screen.findByRole("option", { name: option }));
}

function openSettingsTab(name: string): void {
  fireEvent.mouseDown(screen.getByRole("tab", { name }), { button: 0, ctrlKey: false });
}
