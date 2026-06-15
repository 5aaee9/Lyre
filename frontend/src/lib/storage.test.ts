import { beforeEach, describe, expect, it } from "vitest";
import { resetSettingsStoreForTests } from "./settings-store";
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
    writeNoiseConfig({ provider: "rnnoise", intensity: 0.6, voice_activity_threshold: 0.2 });
    writeAudioProcessingConfig({ echoCancellation: false, autoGainControl: true });

    expect(readNickname()).toBe("Ada");
    expect(readNoiseConfig()).toEqual({
      provider: "rnnoise",
      intensity: 0.6,
      voice_activity_threshold: 0.2
    });
    expect(readAudioProcessingConfig()).toEqual({
      echoCancellation: false,
      autoGainControl: true
    });
  });
});
