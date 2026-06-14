import type {
  IceServerConfig as WebrpcIceServerConfig,
  JoinRoomInput as WebrpcJoinRoomInput,
  JoinRoomResponse as WebrpcJoinRoomResponse,
  NoiseCancellationConfig as WebrpcNoiseCancellationConfig,
  RoomSnapshot as WebrpcRoomSnapshot,
  UserProfile as WebrpcUserProfile
} from "./lyre.gen";
import { NoiseProvider as WebrpcNoiseProvider } from "./lyre.gen";
import { runtimeConfig } from "./runtime-config";

export type NoiseProvider = "off" | "rnnoise" | "deepfilternet";

export function parseNoiseProvider(value: string): NoiseProvider {
  if (value === "rnnoise" || value === "deepfilternet") {
    return value;
  }
  return "off";
}

export function generatedNoiseProviderToRest(provider: WebrpcNoiseProvider): NoiseProvider {
  switch (provider) {
    case WebrpcNoiseProvider.RNNOISE:
      return "rnnoise";
    case WebrpcNoiseProvider.DEEPFILTERNET:
      return "deepfilternet";
    case WebrpcNoiseProvider.OFF:
      return "off";
  }
}

export type NoiseCancellationConfig = Omit<WebrpcNoiseCancellationConfig, "provider" | "voiceActivityThreshold"> & {
  provider: NoiseProvider;
  voice_activity_threshold: WebrpcNoiseCancellationConfig["voiceActivityThreshold"];
};

export type IceServerConfig = Omit<WebrpcIceServerConfig, "username" | "credential"> & {
  username?: WebrpcIceServerConfig["username"] | null;
  credential?: WebrpcIceServerConfig["credential"] | null;
};

export type UserProfile = Omit<WebrpcUserProfile, "joinedAt" | "noise"> & {
  joined_at: string;
  noise: NoiseCancellationConfig;
};

export type RoomSnapshot = Omit<WebrpcRoomSnapshot, "roomID" | "users"> & {
  room_id: string;
  users: UserProfile[];
};

export type JoinRoomResponse = Omit<WebrpcJoinRoomResponse, "user" | "room"> & {
  user: UserProfile;
  room: RoomSnapshot;
};

export type JoinRoomInput = Omit<WebrpcJoinRoomInput, "noise"> & {
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
