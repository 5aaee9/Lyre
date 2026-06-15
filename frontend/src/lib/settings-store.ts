import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import type { NoiseCancellationConfig } from "./api";

export type AudioProcessingConfig = {
  echoCancellation: boolean;
  autoGainControl: boolean;
  noiseSuppression: boolean;
};

type SettingsState = {
  rememberRoom: boolean;
  roomId: string;
  nickname: string;
  noise: NoiseCancellationConfig;
  audioProcessing: AudioProcessingConfig;
  setRememberRoom: (rememberRoom: boolean) => void;
  setRoomId: (roomId: string) => void;
  setNickname: (nickname: string) => void;
  setNoise: (noise: NoiseCancellationConfig) => void;
  setAudioProcessing: (audioProcessing: AudioProcessingConfig) => void;
};

export const defaultNoiseConfig: NoiseCancellationConfig = {
  provider: "off",
  intensity: 0.5,
  voice_activity_threshold: 0.35
};

export const defaultAudioProcessingConfig: AudioProcessingConfig = {
  echoCancellation: true,
  autoGainControl: true,
  noiseSuppression: false
};

export const defaultSettingsState = {
  rememberRoom: false,
  roomId: "DEFAULT",
  nickname: "",
  noise: defaultNoiseConfig,
  audioProcessing: defaultAudioProcessingConfig
};

export const useSettingsStore = create<SettingsState>()(
  persist(
    (set) => ({
      ...defaultSettingsState,
      setRememberRoom: (rememberRoom) => set({ rememberRoom }),
      setRoomId: (roomId) => set({ roomId }),
      setNickname: (nickname) => set({ nickname }),
      setNoise: (noise) => set({ noise }),
      setAudioProcessing: (audioProcessing) => set({ audioProcessing })
    }),
    {
      name: "lyre.settings",
      storage: createJSONStorage(() => localStorage)
    }
  )
);

export function readSettingsSnapshot() {
  return useSettingsStore.getState();
}

export function resetSettingsStoreForTests(): void {
  useSettingsStore.setState(defaultSettingsState);
  useSettingsStore.persist.clearStorage();
}
