import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import Home from "./page";
import { joinRoom } from "@/lib/api";
import { writeNoiseConfig } from "@/lib/storage";

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
    navigation.push.mockClear();
    vi.mocked(joinRoom).mockResolvedValue({
      user: {
        id: "user_a",
        nickname: "User A",
        joined_at: "2026-06-14T00:00:00Z",
        noise: { provider: "deepfilternet", intensity: 0.8, voice_activity_threshold: 0.15 }
      },
      room: {
        room_id: "DEFAULT",
        users: []
      }
    });
  });

  it("keeps stored noise numeric parameters when changing provider before joining", async () => {
    writeNoiseConfig({ provider: "rnnoise", intensity: 0.8, voice_activity_threshold: 0.15 });

    render(<Home />);

    fireEvent.change(screen.getByLabelText("Noise cancellation"), { target: { value: "deepfilternet" } });
    fireEvent.click(screen.getByText("Join"));

    await waitFor(() => {
      expect(joinRoom).toHaveBeenCalledWith("DEFAULT", {
        nickname: "",
        noise: { provider: "deepfilternet", intensity: 0.8, voice_activity_threshold: 0.15 }
      });
    });
  });
});
