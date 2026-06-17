import type { NoiseCancellationConfig } from "./api";
import {
  defaultAudioProcessingConfig,
  defaultAudioDeviceConfig,
  readSettingsSnapshot,
  type AudioDeviceConfig,
  type AudioProcessingConfig,
  useSettingsStore
} from "./settings-store";

export function readRoomId(): string {
  return readSettingsSnapshot().roomId;
}

export function writeRoomId(roomId: string): void {
  useSettingsStore.getState().setRoomId(roomId);
}

export function readRememberRoom(): boolean {
  return readSettingsSnapshot().rememberRoom;
}

export function writeRememberRoom(value: boolean): void {
  useSettingsStore.getState().setRememberRoom(value);
}

export function readNickname(): string {
  return readSettingsSnapshot().nickname;
}

export function writeNickname(nickname: string): void {
  useSettingsStore.getState().setNickname(nickname);
}

export function readNoiseConfig(): NoiseCancellationConfig {
  return readSettingsSnapshot().noise;
}

export function writeNoiseConfig(config: NoiseCancellationConfig): void {
  useSettingsStore.getState().setNoise(config);
}

export function readAudioProcessingConfig(): AudioProcessingConfig {
  return {
    ...defaultAudioProcessingConfig,
    ...readSettingsSnapshot().audioProcessing
  };
}

export function writeAudioProcessingConfig(config: AudioProcessingConfig): void {
  useSettingsStore.getState().setAudioProcessing(config);
}

export function readAudioDeviceConfig(): AudioDeviceConfig {
  return {
    ...defaultAudioDeviceConfig,
    ...readSettingsSnapshot().audioDevices
  };
}

export function writeAudioDeviceConfig(config: AudioDeviceConfig): void {
  useSettingsStore.getState().setAudioDevices(config);
}
