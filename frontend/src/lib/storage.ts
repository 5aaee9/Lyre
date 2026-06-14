import type { NoiseCancellationConfig } from "./api";

const ROOM_ID_KEY = "lyre.roomId";
const REMEMBER_ROOM_KEY = "lyre.rememberRoom";
const NICKNAME_KEY = "lyre.nickname";
const NOISE_KEY = "lyre.noise";

export function readRoomId(): string {
  if (!hasStorage()) {
    return "DEFAULT";
  }
  return localStorage.getItem(ROOM_ID_KEY) ?? "DEFAULT";
}

export function writeRoomId(roomId: string): void {
  if (!hasStorage()) {
    return;
  }
  localStorage.setItem(ROOM_ID_KEY, roomId);
}

export function readRememberRoom(): boolean {
  if (!hasStorage()) {
    return false;
  }
  return localStorage.getItem(REMEMBER_ROOM_KEY) === "true";
}

export function writeRememberRoom(value: boolean): void {
  if (!hasStorage()) {
    return;
  }
  localStorage.setItem(REMEMBER_ROOM_KEY, String(value));
}

export function readNickname(): string {
  if (!hasStorage()) {
    return "";
  }
  return localStorage.getItem(NICKNAME_KEY) ?? "";
}

export function writeNickname(nickname: string): void {
  if (!hasStorage()) {
    return;
  }
  localStorage.setItem(NICKNAME_KEY, nickname);
}

export function readNoiseConfig(): NoiseCancellationConfig {
  if (!hasStorage()) {
    return { provider: "off", intensity: 0.5, voice_activity_threshold: 0.35 };
  }
  const raw = localStorage.getItem(NOISE_KEY);
  if (!raw) {
    return { provider: "off", intensity: 0.5, voice_activity_threshold: 0.35 };
  }
  return JSON.parse(raw) as NoiseCancellationConfig;
}

export function writeNoiseConfig(config: NoiseCancellationConfig): void {
  if (!hasStorage()) {
    return;
  }
  localStorage.setItem(NOISE_KEY, JSON.stringify(config));
}

function hasStorage(): boolean {
  return typeof localStorage !== "undefined";
}
