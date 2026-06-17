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
      audioProcessing: defaultAudioProcessingConfig,
      userAudio: {}
    });
  });

  it("defaults server noise cancelling to DPDFNet high-resolution model", () => {
    expect(readSettingsSnapshot().noise).toEqual({
      provider: "dpdfnet",
      intensity: 0.5,
      voice_activity_threshold: 0.35,
      dpdfnet: {
        model: "dpdfnet8_48khz_hr"
      }
    });
  });

  it("persists settings through the Zustand store", () => {
    useSettingsStore.getState().setRoomId("Team");
    useSettingsStore.getState().setRememberRoom(true);
    useSettingsStore.getState().setNickname("Ada");
    useSettingsStore.getState().setNoise({
      provider: "rnnoise",
      intensity: 0.6,
      voice_activity_threshold: 0.2,
      dpdfnet: defaultNoiseConfig.dpdfnet
    });
    useSettingsStore.getState().setAudioProcessing({
      echoCancellation: false,
      autoGainControl: true,
      noiseSuppression: true
    });
    useSettingsStore.getState().setUserAudioSettings("user_a", {
      muted: true,
      volumePercent: 125
    });

    expect(JSON.parse(localStorage.getItem("lyre.settings") ?? "{}")).toMatchObject({
      state: {
        rememberRoom: true,
        roomId: "Team",
        nickname: "Ada",
        noise: {
          provider: "rnnoise",
          intensity: 0.6,
          voice_activity_threshold: 0.2,
          dpdfnet: defaultNoiseConfig.dpdfnet
        },
        audioProcessing: {
          echoCancellation: false,
          autoGainControl: true,
          noiseSuppression: true
        },
        userAudio: {
          user_a: {
            muted: true,
            volumePercent: 125
          }
        }
      }
    });
  });

  it("clamps per-user audio volume settings", () => {
    useSettingsStore.getState().setUserAudioSettings("quiet", { volumePercent: -5 });
    useSettingsStore.getState().setUserAudioSettings("loud", { volumePercent: 175 });

    expect(readSettingsSnapshot().userAudio.quiet.volumePercent).toBe(0);
    expect(readSettingsSnapshot().userAudio.loud.volumePercent).toBe(150);
  });

  it("clears one user's audio settings", () => {
    useSettingsStore.getState().setUserAudioSettings("user_a", { muted: true });
    useSettingsStore.getState().clearUserAudioSettings("user_a");

    expect(readSettingsSnapshot().userAudio.user_a).toBeUndefined();
  });

  it("hydrates legacy noise settings with DPDFNet defaults", async () => {
    localStorage.setItem(
      "lyre.settings",
      JSON.stringify({
        state: {
          noise: {
            provider: "dpdfnet",
            intensity: 0.6,
            voice_activity_threshold: 0.2
          }
        }
      })
    );

    await useSettingsStore.persist.rehydrate();

    expect(readSettingsSnapshot().noise).toEqual({
      provider: "dpdfnet",
      intensity: 0.6,
      voice_activity_threshold: 0.2,
      dpdfnet: defaultNoiseConfig.dpdfnet
    });
  });
});
