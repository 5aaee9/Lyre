import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import type { NoiseCancellationConfig } from "./api";

export type AudioProcessingConfig = {
  echoCancellation: boolean;
  autoGainControl: boolean;
  noiseSuppression: boolean;
};

export type AudioDeviceConfig = {
  inputDeviceId: string;
  outputDeviceId: string;
};

export type UserAudioSettings = {
  muted: boolean;
  volumePercent: number;
};

export type SettingsState = {
  rememberRoom: boolean;
  roomId: string;
  nickname: string;
  audioDiagnosticsEnabled: boolean;
  noise: NoiseCancellationConfig;
  audioProcessing: AudioProcessingConfig;
  audioDevices: AudioDeviceConfig;
  userAudio: Record<string, UserAudioSettings>;
  setRememberRoom: (rememberRoom: boolean) => void;
  setRoomId: (roomId: string) => void;
  setNickname: (nickname: string) => void;
  setAudioDiagnosticsEnabled: (audioDiagnosticsEnabled: boolean) => void;
  setNoise: (noise: NoiseCancellationConfig) => void;
  setAudioProcessing: (audioProcessing: AudioProcessingConfig) => void;
  setAudioDevices: (audioDevices: AudioDeviceConfig) => void;
  setUserAudioSettings: (userId: string, settings: Partial<UserAudioSettings>) => void;
  clearUserAudioSettings: (userId: string) => void;
};

export type SettingsSnapshot = SettingsState;

export const defaultNoiseConfig: NoiseCancellationConfig = {
  provider: "dpdfnet",
  intensity: 0.5,
  voice_activity_threshold: 0.35,
  dpdfnet: {
    model: "dpdfnet2_48khz_hr"
  }
};

export const defaultAudioProcessingConfig: AudioProcessingConfig = {
  echoCancellation: true,
  autoGainControl: true,
  noiseSuppression: true
};

export const defaultAudioDeviceConfig: AudioDeviceConfig = {
  inputDeviceId: "",
  outputDeviceId: ""
};

export const defaultSettingsState = {
  rememberRoom: false,
  roomId: "DEFAULT",
  nickname: "",
  audioDiagnosticsEnabled: false,
  noise: defaultNoiseConfig,
  audioProcessing: defaultAudioProcessingConfig,
  audioDevices: defaultAudioDeviceConfig,
  userAudio: {}
};

function clampUserVolume(volume: number): number {
  return Math.min(150, Math.max(0, volume));
}

function mergeSettingsState(persistedState: unknown, currentState: SettingsState): SettingsState {
  const persisted = persistedState as Partial<SettingsState>;

  return {
    ...currentState,
    ...persisted,
    noise: {
      ...defaultNoiseConfig,
      ...persisted.noise,
      dpdfnet: {
        ...defaultNoiseConfig.dpdfnet,
        ...persisted.noise?.dpdfnet
      }
    },
    audioProcessing: {
      ...defaultAudioProcessingConfig,
      ...persisted.audioProcessing
    },
    audioDevices: {
      ...defaultAudioDeviceConfig,
      ...persisted.audioDevices
    },
    userAudio: persisted.userAudio ?? {}
  };
}

export const useSettingsStore = create<SettingsState>()(
  persist(
    (set) => ({
      ...defaultSettingsState,
      setRememberRoom: (rememberRoom) => set({ rememberRoom }),
      setRoomId: (roomId) => set({ roomId }),
      setNickname: (nickname) => set({ nickname }),
      setAudioDiagnosticsEnabled: (audioDiagnosticsEnabled) => set({ audioDiagnosticsEnabled }),
      setNoise: (noise) => set({ noise }),
      setAudioProcessing: (audioProcessing) => set({ audioProcessing }),
      setAudioDevices: (audioDevices) => set({ audioDevices }),
      setUserAudioSettings: (userId, settings) =>
        set((state) => {
          const current = state.userAudio[userId] ?? { muted: false, volumePercent: 100 };
          return {
            userAudio: {
              ...state.userAudio,
              [userId]: {
                ...current,
                ...settings,
                volumePercent:
                  settings.volumePercent === undefined
                    ? current.volumePercent
                    : clampUserVolume(settings.volumePercent)
              }
            }
          };
        }),
      clearUserAudioSettings: (userId) =>
        set((state) => {
          const userAudio = { ...state.userAudio };
          delete userAudio[userId];
          return { userAudio };
        })
    }),
    {
      name: "lyre.settings",
      storage: createJSONStorage(() => localStorage),
      merge: mergeSettingsState
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
