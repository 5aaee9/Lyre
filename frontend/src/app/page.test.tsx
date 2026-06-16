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
    fireEvent.click(screen.getByText("Join"));

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
});
