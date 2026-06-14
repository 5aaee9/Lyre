import { beforeEach, describe, expect, it } from "vitest";
import { readNickname, readNoiseConfig, readRememberRoom, readRoomId, writeNickname, writeNoiseConfig, writeRememberRoom, writeRoomId } from "./storage";

describe("storage", () => {
  beforeEach(() => localStorage.clear());

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

    expect(readNickname()).toBe("Ada");
    expect(readNoiseConfig().provider).toBe("rnnoise");
  });
});
