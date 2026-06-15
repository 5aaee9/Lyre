import type {
  IceServerConfig as WebrpcIceServerConfig,
  JoinRoomInput as WebrpcJoinRoomInput,
  JoinRoomResponse as WebrpcJoinRoomResponse,
  MediaTopology as WebrpcMediaTopology,
  MediaRelayParticipant as WebrpcMediaRelayParticipant,
  MediaRelayRoomStatus as WebrpcMediaRelayRoomStatus,
  MediaRelayTrack as WebrpcMediaRelayTrack,
  NoiseCancellationConfig as WebrpcNoiseCancellationConfig,
  RoomSnapshot as WebrpcRoomSnapshot,
  ServerMediaAnswer as WebrpcServerMediaAnswer,
  ClosedServerMediaSession as WebrpcClosedServerMediaSession,
  ServerMediaIceCandidate as WebrpcServerMediaIceCandidate,
  ServerMediaSessionStatus as WebrpcServerMediaSessionStatus,
  UserProfile as WebrpcUserProfile
} from "./lyre.gen";
import { MediaTopologyMode as WebrpcMediaTopologyMode } from "./lyre.gen";
import { MediaRelayMode as WebrpcMediaRelayMode } from "./lyre.gen";
import { MediaRelayStatus as WebrpcMediaRelayStatus } from "./lyre.gen";
import { MediaTrackKind as WebrpcMediaTrackKind } from "./lyre.gen";
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

export type MediaTopologyMode = "p2p_mesh" | "media_relay";

export function generatedMediaTopologyModeToRest(mode: WebrpcMediaTopologyMode): MediaTopologyMode {
  switch (mode) {
    case WebrpcMediaTopologyMode.MEDIA_RELAY:
      return "media_relay";
    case WebrpcMediaTopologyMode.P2P_MESH:
      return "p2p_mesh";
  }
}

export type MediaRelayStatus = "inactive" | "active";
export type MediaRelayMode = "p2p_mesh" | "media_relay";
export type MediaTrackKind = "audio" | "video";
export type ServerMediaSessionState = "new" | "negotiating" | "connected" | "closed";

export function generatedMediaRelayStatusToRest(status: WebrpcMediaRelayStatus): MediaRelayStatus {
  switch (status) {
    case WebrpcMediaRelayStatus.ACTIVE:
      return "active";
    case WebrpcMediaRelayStatus.INACTIVE:
      return "inactive";
  }
}

export function generatedMediaRelayModeToRest(mode: WebrpcMediaRelayMode): MediaRelayMode {
  switch (mode) {
    case WebrpcMediaRelayMode.MEDIA_RELAY:
      return "media_relay";
    case WebrpcMediaRelayMode.P2P_MESH:
      return "p2p_mesh";
  }
}

export function generatedMediaTrackKindToRest(kind: WebrpcMediaTrackKind): MediaTrackKind {
  switch (kind) {
    case WebrpcMediaTrackKind.AUDIO:
      return "audio";
    case WebrpcMediaTrackKind.VIDEO:
      return "video";
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

export type MediaTopology = Omit<
  WebrpcMediaTopology,
  | "mode"
  | "turnRelaySupported"
  | "serverSideAudioProcessing"
  | "serverSideNoiseCancelling"
  | "serverNoiseCancellingRequires"
> & {
  mode: MediaTopologyMode;
  turn_relay_supported: WebrpcMediaTopology["turnRelaySupported"];
  server_side_audio_processing: WebrpcMediaTopology["serverSideAudioProcessing"];
  server_side_noise_cancelling: WebrpcMediaTopology["serverSideNoiseCancelling"];
  server_noise_cancelling_requires: MediaTopologyMode;
};

export type MediaRelayTrack = Omit<WebrpcMediaRelayTrack, "trackID" | "kind"> & {
  track_id: string;
  kind: MediaTrackKind;
};

export type MediaRelayParticipant = Omit<WebrpcMediaRelayParticipant, "userID" | "tracks"> & {
  user_id: string;
  tracks: MediaRelayTrack[];
};

export type MediaRelayRoomStatus = Omit<
  WebrpcMediaRelayRoomStatus,
  | "roomID"
  | "status"
  | "mode"
  | "serverSideAudioProcessing"
  | "serverSideNoiseCancelling"
  | "noise"
  | "participants"
> & {
  room_id: string;
  status: MediaRelayStatus;
  mode: MediaRelayMode;
  server_side_audio_processing: WebrpcMediaRelayRoomStatus["serverSideAudioProcessing"];
  server_side_noise_cancelling: WebrpcMediaRelayRoomStatus["serverSideNoiseCancelling"];
  noise: NoiseCancellationConfig;
  participants: MediaRelayParticipant[];
};

export type ServerMediaAnswer = Omit<
  WebrpcServerMediaAnswer,
  "roomID" | "userID" | "audioTrackID" | "state"
> & {
  room_id: string;
  user_id: string;
  audio_track_id: string;
  state: ServerMediaSessionState;
};

export type ServerMediaSessionStatus = Omit<
  WebrpcServerMediaSessionStatus,
  "roomID" | "userID" | "audioTrackID" | "state"
> & {
  room_id: string;
  user_id: string;
  audio_track_id: string;
  state: ServerMediaSessionState;
};

export type CloseServerMediaSessionResponse = Omit<
  WebrpcClosedServerMediaSession,
  "mediaRelay" | "session"
> & {
  media_relay: MediaRelayRoomStatus;
  session?: ServerMediaSessionStatus | null;
};

export type ServerMediaIceCandidate = Omit<
  WebrpcServerMediaIceCandidate,
  "roomID" | "userID" | "sdpMid" | "sdpMLineIndex" | "usernameFragment"
> & {
  room_id: string;
  user_id: string;
  sdp_mid?: string | null;
  sdp_mline_index?: number | null;
  username_fragment?: string | null;
};

export type UserProfile = Omit<WebrpcUserProfile, "joinedAt" | "noise"> & {
  joined_at: string;
  noise: NoiseCancellationConfig;
};

export type RoomSnapshot = Omit<WebrpcRoomSnapshot, "roomID" | "users"> & {
  room_id: string;
  users: UserProfile[];
};

export type JoinRoomResponse = Omit<WebrpcJoinRoomResponse, "user" | "room" | "accessToken"> & {
  access_token: string;
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

export function mediaRelayUrl(roomId: string): string {
  return `${roomUrl(roomId)}/media-relay`;
}

export function serverMediaOfferUrl(roomId: string): string {
  return `${roomUrl(roomId)}/server-media/offer`;
}

export function serverMediaCandidatesUrl(roomId: string): string {
  return `${roomUrl(roomId)}/server-media/candidates`;
}

export function serverMediaCloseUrl(roomId: string): string {
  return `${roomUrl(roomId)}/server-media/close`;
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

function bearerHeaders(accessToken: string): Record<string, string> {
  return { authorization: `Bearer ${accessToken}` };
}

export async function leaveRoom(roomId: string, userId: string, accessToken: string): Promise<RoomSnapshot> {
  const response = await fetch(`${roomUrl(roomId)}/leave`, {
    method: "POST",
    headers: { ...bearerHeaders(accessToken), "content-type": "application/json" },
    body: JSON.stringify({ user_id: userId })
  });
  return response.json();
}

export async function getMediaRelay(roomId: string): Promise<MediaRelayRoomStatus> {
  const response = await fetch(mediaRelayUrl(roomId));
  return jsonOrThrow(response, "failed to load media relay");
}

export async function startMediaRelay(
  roomId: string,
  noise: NoiseCancellationConfig | undefined,
  accessToken: string
): Promise<MediaRelayRoomStatus> {
  const response = await fetch(`${mediaRelayUrl(roomId)}/start`, {
    method: "POST",
    headers: { ...bearerHeaders(accessToken), "content-type": "application/json" },
    body: JSON.stringify({ noise })
  });
  return jsonOrThrow(response, "failed to start media relay");
}

export async function stopMediaRelay(
  roomId: string,
  userId: string,
  accessToken: string
): Promise<MediaRelayRoomStatus> {
  const response = await fetch(`${mediaRelayUrl(roomId)}/stop`, {
    method: "POST",
    headers: { ...bearerHeaders(accessToken), "content-type": "application/json" },
    body: JSON.stringify({ user_id: userId })
  });
  return jsonOrThrow(response, "failed to stop media relay");
}

export async function registerMediaTrack(
  roomId: string,
  userId: string,
  trackId: string,
  kind: MediaTrackKind,
  accessToken: string
): Promise<MediaRelayRoomStatus> {
  const response = await fetch(`${mediaRelayUrl(roomId)}/tracks`, {
    method: "POST",
    headers: { ...bearerHeaders(accessToken), "content-type": "application/json" },
    body: JSON.stringify({ user_id: userId, track_id: trackId, kind })
  });
  return jsonOrThrow(response, "failed to register media track");
}

export async function answerServerMediaOffer(
  roomId: string,
  userId: string,
  audioTrackId: string,
  sdp: string,
  accessToken: string
): Promise<ServerMediaAnswer> {
  const response = await fetch(serverMediaOfferUrl(roomId), {
    method: "POST",
    headers: { ...bearerHeaders(accessToken), "content-type": "application/json" },
    body: JSON.stringify({ user_id: userId, audio_track_id: audioTrackId, sdp })
  });
  return jsonOrThrow(response, "failed to negotiate server media offer");
}

export async function addServerMediaIceCandidate(
  roomId: string,
  candidate: Omit<ServerMediaIceCandidate, "room_id">,
  accessToken: string
): Promise<ServerMediaIceCandidate> {
  const response = await fetch(serverMediaCandidatesUrl(roomId), {
    method: "POST",
    headers: { ...bearerHeaders(accessToken), "content-type": "application/json" },
    body: JSON.stringify(candidate)
  });
  return jsonOrThrow(response, "failed to add server media ICE candidate");
}

export async function getServerMediaIceCandidates(
  roomId: string,
  userId: string,
  accessToken: string
): Promise<ServerMediaIceCandidate[]> {
  const query = new URLSearchParams({ user_id: userId });
  const response = await fetch(`${serverMediaCandidatesUrl(roomId)}?${query.toString()}`, {
    headers: bearerHeaders(accessToken)
  });
  return jsonOrThrow(response, "failed to load server media ICE candidates");
}

export async function closeServerMediaSession(
  roomId: string,
  userId: string,
  accessToken: string
): Promise<CloseServerMediaSessionResponse> {
  const response = await fetch(serverMediaCloseUrl(roomId), {
    method: "POST",
    headers: { ...bearerHeaders(accessToken), "content-type": "application/json" },
    body: JSON.stringify({ user_id: userId })
  });
  return jsonOrThrow(response, "failed to close server media session");
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

export async function getMediaTopology(): Promise<MediaTopology> {
  const response = await fetch(`${apiBaseUrl()}/api/webrtc/topology`);
  return response.json();
}

export function appBaseUrl(): string {
  return runtimeConfig().appBaseUrl;
}

export function shareRoomUrl(roomId: string): string {
  return `${appBaseUrl()}/room/${encodeURIComponent(roomId)}`;
}

async function jsonOrThrow<T>(response: Response, message: string): Promise<T> {
  if (!response.ok) {
    throw new Error(`${message}: ${response.status}`);
  }
  return response.json();
}
