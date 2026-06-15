import { beforeEach, describe, expect, it } from "vitest";
import { defaultNoiseConfig, resetSettingsStoreForTests } from "./settings-store";
import {
  readAudioProcessingConfig,
  readNickname,
  readNoiseConfig,
  readRememberRoom,
  readRoomId,
  writeAudioProcessingConfig,
  writeNickname,
  writeNoiseConfig,
  writeRememberRoom,
  writeRoomId
} from "./storage";

describe("storage", () => {
  beforeEach(() => {
    localStorage.clear();
    resetSettingsStoreForTests();
  });

  it("stores room and remember flag", () => {
    expect(readRoomId()).toBe("DEFAULT");
    expect(readRememberRoom()).toBe(false);

    writeRoomId("Team");
    writeRememberRoom(true);

    expect(readRoomId()).toBe("Team");
    expect(readRememberRoom()).toBe(true);
  });

  it("stores nickname and noise config", () => {
    writeNickname("Ada");
    writeNoiseConfig({
      provider: "rnnoise",
      intensity: 0.6,
      voice_activity_threshold: 0.2,
      dpdfnet: defaultNoiseConfig.dpdfnet
    });
    writeAudioProcessingConfig({ echoCancellation: false, autoGainControl: true, noiseSuppression: true });

    expect(readNickname()).toBe("Ada");
    expect(readNoiseConfig()).toEqual({
      provider: "rnnoise",
      intensity: 0.6,
      voice_activity_threshold: 0.2,
      dpdfnet: defaultNoiseConfig.dpdfnet
    });
    expect(readAudioProcessingConfig()).toEqual({
      echoCancellation: false,
      autoGainControl: true,
      noiseSuppression: true
    });
  });

  it("fills missing browser audio processing fields from defaults", () => {
    writeAudioProcessingConfig({ echoCancellation: false, autoGainControl: true } as ReturnType<
      typeof readAudioProcessingConfig
    >);

    expect(readAudioProcessingConfig()).toEqual({
      echoCancellation: false,
      autoGainControl: true,
      noiseSuppression: false
    });
  });
});
