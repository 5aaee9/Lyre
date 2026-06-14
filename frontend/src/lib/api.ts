export type NoiseProvider = "off" | "rnnoise" | "deepfilternet";

export function parseNoiseProvider(value: string): NoiseProvider {
  if (value === "rnnoise" || value === "deepfilternet") {
    return value;
  }
  return "off";
}

export type NoiseCancellationConfig = {
  provider: NoiseProvider;
  intensity: number;
  voice_activity_threshold: number;
};

export type IceServerConfig = {
  urls: string[];
  username?: string | null;
  credential?: string | null;
};

export type UserProfile = {
  id: string;
  nickname: string;
  joined_at: string;
  noise: NoiseCancellationConfig;
};

export type RoomSnapshot = {
  room_id: string;
  users: UserProfile[];
};

export type JoinRoomResponse = {
  user: UserProfile;
  room: RoomSnapshot;
};

export type JoinRoomInput = {
  nickname?: string;
  noise?: NoiseCancellationConfig;
};

export function apiBaseUrl(): string {
  return runtimeConfig().appApiUrl;
}

export function roomUrl(roomId: string): string {
  return `${apiBaseUrl()}/api/rooms/${encodeURIComponent(roomId)}`;
}

export async function getRoom(roomId: string): Promise<RoomSnapshot> {
  const response = await fetch(roomUrl(roomId));
  return response.json();
}

export async function joinRoom(roomId: string, input: JoinRoomInput): Promise<JoinRoomResponse> {
  const response = await fetch(`${roomUrl(roomId)}/join`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(input)
  });
  return response.json();
}

export async function leaveRoom(roomId: string, userId: string): Promise<RoomSnapshot> {
  const response = await fetch(`${roomUrl(roomId)}/leave`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ user_id: userId })
  });
  return response.json();
}

export async function getNoiseProviders(): Promise<NoiseCancellationConfig[]> {
  const response = await fetch(`${apiBaseUrl()}/api/noise/providers`);
  return response.json();
}

export async function getIceServers(): Promise<IceServerConfig[]> {
  const response = await fetch(`${apiBaseUrl()}/api/webrtc/ice-servers`);
  if (!response.ok) {
    throw new Error(`failed to load ICE servers: ${response.status}`);
  }
  return response.json();
}

export function appBaseUrl(): string {
  return runtimeConfig().appBaseUrl;
}

export function shareRoomUrl(roomId: string): string {
  return `${appBaseUrl()}/room/${encodeURIComponent(roomId)}`;
}
import { runtimeConfig } from "./runtime-config";
