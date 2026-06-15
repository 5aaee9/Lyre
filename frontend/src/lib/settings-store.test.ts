import { beforeEach, describe, expect, it } from "vitest";
import {
  defaultAudioProcessingConfig,
  defaultNoiseConfig,
  readSettingsSnapshot,
  resetSettingsStoreForTests,
  useSettingsStore
} from "./settings-store";

describe("settings store", () => {
  beforeEach(() => {
    localStorage.clear();
    resetSettingsStoreForTests();
  });

  it("defaults browser audio processing to enabled", () => {
    expect(readSettingsSnapshot()).toMatchObject({
      rememberRoom: false,
      roomId: "DEFAULT",
      nickname: "",
      noise: defaultNoiseConfig,
      audioProcessing: defaultAudioProcessingConfig
    });
  });

  it("persists settings through the Zustand store", () => {
    useSettingsStore.getState().setRoomId("Team");
    useSettingsStore.getState().setRememberRoom(true);
    useSettingsStore.getState().setNickname("Ada");
    useSettingsStore.getState().setNoise({
      provider: "rnnoise",
      intensity: 0.6,
      voice_activity_threshold: 0.2
    });
    useSettingsStore.getState().setAudioProcessing({
      echoCancellation: false,
      autoGainControl: true
    });

    expect(JSON.parse(localStorage.getItem("lyre.settings") ?? "{}")).toMatchObject({
      state: {
        rememberRoom: true,
        roomId: "Team",
        nickname: "Ada",
        noise: {
          provider: "rnnoise",
          intensity: 0.6,
          voice_activity_threshold: 0.2
        },
        audioProcessing: {
          echoCancellation: false,
          autoGainControl: true
        }
      }
    });
  });
});
