import { beforeEach, describe, expect, it } from "vitest";
import {
  defaultAudioProcessingConfig,
  defaultAudioDeviceConfig,
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
      language: "system",
      noise: defaultNoiseConfig,
      audioProcessing: defaultAudioProcessingConfig,
      audioDevices: defaultAudioDeviceConfig,
      userAudio: {}
    });
  });

  it("defaults client noise cancellation to disabled", () => {
    expect(readSettingsSnapshot().audioProcessing.clientNoiseCancellation).toBe(false);
  });

  it("defaults server noise cancelling to DPDFNet high-resolution model", () => {
    expect(readSettingsSnapshot().noise).toEqual({
      provider: "dpdfnet",
      intensity: 0.5,
      voice_activity_threshold: 0.35,
      dpdfnet: {
        model: "dpdfnet2_48khz_hr"
      }
    });
  });

  it("persists settings through the Zustand store", () => {
    useSettingsStore.getState().setRoomId("Team");
    useSettingsStore.getState().setRememberRoom(true);
    useSettingsStore.getState().setNickname("Ada");
    useSettingsStore.getState().setLanguage("zh-CN");
    useSettingsStore.getState().setNoise({
      provider: "rnnoise",
      intensity: 0.6,
      voice_activity_threshold: 0.2,
      dpdfnet: defaultNoiseConfig.dpdfnet
    });
    useSettingsStore.getState().setAudioProcessing({
      echoCancellation: false,
      autoGainControl: true,
      noiseSuppression: true,
      clientNoiseCancellation: true
    });
    useSettingsStore.getState().setAudioDevices({
      inputDeviceId: "mic-a",
      outputDeviceId: "speaker-a"
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
        language: "zh-CN",
        noise: {
          provider: "rnnoise",
          intensity: 0.6,
          voice_activity_threshold: 0.2,
          dpdfnet: defaultNoiseConfig.dpdfnet
        },
        audioProcessing: {
          echoCancellation: false,
          autoGainControl: true,
          noiseSuppression: true,
          clientNoiseCancellation: true
        },
        audioDevices: {
          inputDeviceId: "mic-a",
          outputDeviceId: "speaker-a"
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

  it("syncs explicit language settings to the next-intl locale cookie", () => {
    useSettingsStore.getState().setLanguage("zh-CN");
    expect(document.cookie).toContain("NEXT_LOCALE=zh-CN");

    useSettingsStore.getState().setLanguage("system");
    expect(document.cookie).not.toContain("NEXT_LOCALE=");
  });

  it("syncs persisted language settings to the locale cookie after hydration", async () => {
    localStorage.setItem(
      "lyre.settings",
      JSON.stringify({
        state: {
          language: "zh-CN"
        }
      })
    );

    await useSettingsStore.persist.rehydrate();

    expect(document.cookie).toContain("NEXT_LOCALE=zh-CN");
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
