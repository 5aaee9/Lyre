import { answerServerMediaOffer, type IceServerConfig, type ServerMediaIceCandidate } from "./api";
import type { UserAudioSettings } from "./settings-store";
import { VoiceActivityDetector } from "./voice-activity";

export type ServerMediaAudioSessionInput = {
  roomId: string;
  userId: string;
  accessToken: string;
  socket: WebSocket;
  audioTrackId?: string;
  iceServers: IceServerConfig[];
  stream: MediaStream;
  listenOnly?: boolean;
  subscribedSourceIds?: string[];
  outputDeviceId?: string;
  userAudio?: Record<string, UserAudioSettings>;
  pollIntervalMs?: number;
  onError?: (message: string) => void;
  onPlaybackBlocked?: (blocked: boolean) => void;
  onConnectionInterrupted?: () => void;
  onRemoteTrack?: () => void;
  onRemoteSpeakingChange?: (userId: string, speaking: boolean) => void;
};

export type RemotePlayback = {
  stream: MediaStream;
  source: MediaStreamAudioSourceNode;
  gain: GainNode;
  voiceActivity: VoiceActivityDetector;
  element: HTMLAudioElement;
  settings: UserAudioSettings;
};

export type ServerMediaRemoteSourceDiagnostics = {
  userId: string;
  trackIds: string[];
  gain: number;
  muted: boolean;
  volumePercent: number;
  readyStates: MediaStreamTrackState[];
  enabled: boolean[];
};

export type ServerMediaAudioStats = {
  packetsSent: number;
  bytesSent: number;
  packetsReceived: number;
  bytesReceived: number;
  packetsLost: number;
  remotePacketsLost: number;
  audioLevel: number | null;
  totalAudioEnergy: number | null;
  totalSamplesDuration: number | null;
  roundTripTimeMs: number | null;
};

export type ServerMediaIceDiagnostics = {
  localCandidateCount: number;
  serverCandidateCount: number;
  lastServerCandidateCount: number;
  lastServerCandidateAt: string | null;
  lastLocalCandidateAt: string | null;
  lastServerCandidateError: string | null;
};

export type ServerMediaAudioDiagnostics = {
  connectionState: RTCPeerConnectionState;
  iceConnectionState: RTCIceConnectionState;
  signalingState: RTCSignalingState;
  audioContextState: AudioContextState | "uncreated";
  remoteTrackIds: string[];
  receiverTrackIds: string[];
  onTrackTrackIds: string[];
  rejectedTrackIds: string[];
  ice: ServerMediaIceDiagnostics;
  remoteSources: ServerMediaRemoteSourceDiagnostics[];
  lastPlaybackError: string | null;
  stats: ServerMediaAudioStats;
};

export type DtxEncodingParameters = RTCRtpEncodingParameters & {
  dtx?: "disabled" | "enabled";
};

export type DtxRtpSendParameters = RTCRtpSendParameters & {
  encodings: DtxEncodingParameters[];
};

export type AudioContextWithSinkId = AudioContext & {
  setSinkId?: (sinkId: string) => Promise<void>;
};

export const DEFAULT_AUDIO_TRACK_ID = "audio-main";
export const DEFAULT_CANDIDATE_POLL_INTERVAL_MS = 1_000;
export const ICE_DISCONNECTED_GRACE_MS = 4_000;
export const PLAYBACK_RESUME_CHECK_MS = 1_000;
export const SOCKET_NOT_CONNECTED_ERROR = "Audio signalling websocket is not connected";

const EMPTY_AUDIO_STATS: ServerMediaAudioStats = {
  packetsSent: 0,
  bytesSent: 0,
  packetsReceived: 0,
  bytesReceived: 0,
  packetsLost: 0,
  remotePacketsLost: 0,
  audioLevel: null,
  totalAudioEnergy: null,
  totalSamplesDuration: null,
  roundTripTimeMs: null
};

export function createRemotePlayback(
  track: MediaStreamTrack,
  sourceUserId: string,
  audioContext: AudioContext,
  onSpeakingChange: ServerMediaAudioSessionInput["onRemoteSpeakingChange"],
  reportPlaybackError: (error: unknown) => void
): RemotePlayback {
  const stream = new MediaStream();
  stream.addTrack(track);
  const element = document.createElement("audio");
  element.autoplay = true;
  element.hidden = true;
  element.muted = true;
  element.srcObject = stream;
  document.body.append(element);
  const source = audioContext.createMediaStreamSource(stream);
  const voiceActivity = new VoiceActivityDetector(stream, (speaking) => {
    onSpeakingChange?.(sourceUserId, speaking);
  });
  voiceActivity.start();
  const gain = audioContext.createGain();
  source.connect(gain);
  gain.connect(audioContext.destination);
  if (audioContext.state === "suspended") {
    void audioContext.resume().catch(reportPlaybackError);
  }
  void element.play().catch(reportPlaybackError);
  return {
    stream,
    source,
    gain,
    voiceActivity,
    element,
    settings: { muted: false, volumePercent: 100 }
  };
}

export function applyRemotePlaybackSettings(
  playback: RemotePlayback | undefined,
  settings: UserAudioSettings
): void {
  if (!playback) {
    return;
  }
  const volumePercent = Math.min(150, Math.max(0, settings.volumePercent));
  playback.gain.gain.value = settings.muted ? 0 : volumePercent / 100;
  playback.settings = { muted: settings.muted, volumePercent };
}

export function parseServerMediaSourceTrackId(trackId: string): string | null {
  const prefix = "lyre-user:";
  const suffix = ":audio";
  if (!trackId.startsWith(prefix) || !trackId.endsWith(suffix)) {
    return null;
  }
  const encoded = trackId.slice(prefix.length, -suffix.length);
  try {
    return decodeURIComponent(encoded);
  } catch {
    return null;
  }
}

export function parseServerMediaSourceTrackIdsByMid(sdp: string): Map<string, string> {
  const tracksByMid = new Map<string, string>();
  let currentMid: string | null = null;
  for (const rawLine of sdp.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (line.startsWith("m=")) {
      currentMid = null;
      continue;
    }
    if (line.startsWith("a=mid:")) {
      currentMid = line.slice("a=mid:".length);
      continue;
    }
    if (!currentMid || !line.startsWith("a=msid:")) {
      continue;
    }
    const [, trackId] = line.slice("a=msid:".length).split(/\s+/, 2);
    if (trackId && parseServerMediaSourceTrackId(trackId)) {
      tracksByMid.set(currentMid, trackId);
    }
  }
  return tracksByMid;
}

export function candidateKey(candidate: ServerMediaIceCandidate): string {
  return [
    candidate.candidate,
    candidate.sdp_mid ?? "",
    candidate.sdp_mline_index ?? "",
    candidate.username_fragment ?? ""
  ].join("|");
}

export function serverIceCandidateInit(candidate: ServerMediaIceCandidate): RTCIceCandidateInit {
  return {
    candidate: candidate.candidate,
    sdpMid: candidate.sdp_mid ?? undefined,
    sdpMLineIndex: candidate.sdp_mline_index ?? undefined,
    usernameFragment: candidate.username_fragment ?? undefined
  };
}

export function errorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error) {
    return error.message;
  }
  return typeof error === "string" ? error : fallback;
}

export function summarizeAudioStats(report: RTCStatsReport): ServerMediaAudioStats {
  const summary = { ...EMPTY_AUDIO_STATS };
  for (const stat of report.values()) {
    if (!isAudioRtpStats(stat)) {
      continue;
    }
    if (stat.type === "outbound-rtp") {
      summary.packetsSent += stat.packetsSent ?? 0;
      summary.bytesSent += stat.bytesSent ?? 0;
    }
    if (stat.type === "inbound-rtp") {
      summary.packetsReceived += stat.packetsReceived ?? 0;
      summary.bytesReceived += stat.bytesReceived ?? 0;
      summary.packetsLost += stat.packetsLost ?? 0;
      if (typeof stat.audioLevel === "number") {
        summary.audioLevel = Math.max(summary.audioLevel ?? 0, stat.audioLevel);
      }
      if (typeof stat.totalAudioEnergy === "number") {
        summary.totalAudioEnergy = (summary.totalAudioEnergy ?? 0) + stat.totalAudioEnergy;
      }
      if (typeof stat.totalSamplesDuration === "number") {
        summary.totalSamplesDuration = (summary.totalSamplesDuration ?? 0) + stat.totalSamplesDuration;
      }
    }
    if (stat.type === "remote-inbound-rtp") {
      summary.remotePacketsLost += stat.packetsLost ?? 0;
      if (typeof stat.roundTripTime === "number") {
        summary.roundTripTimeMs = Math.round(stat.roundTripTime * 1000);
      }
    }
  }
  return summary;
}

type AudioRtpStats = RTCStats & {
  kind?: string;
  packetsSent?: number;
  bytesSent?: number;
  packetsReceived?: number;
  bytesReceived?: number;
  packetsLost?: number;
  audioLevel?: number;
  totalAudioEnergy?: number;
  totalSamplesDuration?: number;
  roundTripTime?: number;
};

function isAudioRtpStats(stat: RTCStats): stat is AudioRtpStats {
  if (stat.type !== "inbound-rtp" && stat.type !== "outbound-rtp" && stat.type !== "remote-inbound-rtp") {
    return false;
  }
  return !("kind" in stat) || stat.kind === "audio";
}

export function applyOutputDevice(audioContext: AudioContext, outputDeviceId: string | undefined): Promise<void> {
  const setSinkId = (audioContext as AudioContextWithSinkId).setSinkId;
  if (!outputDeviceId || !setSinkId) {
    return Promise.resolve();
  }
  return setSinkId.call(audioContext, outputDeviceId);
}

export async function enableOpusDtx(peer: RTCPeerConnection): Promise<void> {
  const senders = peer.getSenders().filter((sender) => sender.track?.kind === "audio");
  await Promise.all(senders.map(async (sender) => {
    try {
      const parameters = sender.getParameters() as DtxRtpSendParameters;
      const encodings = parameters.encodings.length > 0 ? parameters.encodings : [{}];
      await sender.setParameters({
        ...parameters,
        encodings: encodings.map((encoding) => ({ ...encoding, dtx: "enabled" }))
      });
    } catch {
      return;
    }
  }));
}

export async function negotiateServerMediaOffer(
  peer: RTCPeerConnection,
  input: ServerMediaAudioSessionInput,
  audioTrackId: string,
  sourceTrackIdsByMid: Map<string, string>
): Promise<void> {
  if (peer.signalingState !== "stable") {
    throw new Error(`Cannot negotiate server media audio in signaling state ${peer.signalingState}`);
  }
  const offer = await peer.createOffer();
  await peer.setLocalDescription(offer);
  await enableOpusDtx(peer);
  const answer = await answerServerMediaOffer(input.roomId, input.userId, audioTrackId, offer.sdp ?? "", input.accessToken);
  sourceTrackIdsByMid.clear();
  for (const [mid, trackId] of parseServerMediaSourceTrackIdsByMid(answer.sdp)) {
    sourceTrackIdsByMid.set(mid, trackId);
  }
  // ontrack can fire during setRemoteDescription, so the mid map must be current first.
  await peer.setRemoteDescription({ type: "answer", sdp: answer.sdp });
}

export function ensureRecvonlyAudioTransceivers(peer: RTCPeerConnection, count: number): void {
  const existing = peer.getTransceivers().filter((transceiver) => transceiver.receiver.track.kind === "audio").length;
  for (let index = existing; index < count; index += 1) {
    peer.addTransceiver("audio", { direction: "recvonly" });
  }
}

export function remoteSourceDiagnostics(remotePlayback: Map<string, RemotePlayback>): ServerMediaRemoteSourceDiagnostics[] {
  return [...remotePlayback.entries()]
    .map(([userId, playback]) => {
      const tracks = playback.stream.getAudioTracks();
      return {
        userId,
        trackIds: tracks.map((track) => track.id),
        gain: playback.gain.gain.value,
        muted: playback.settings.muted,
        volumePercent: playback.settings.volumePercent,
        readyStates: tracks.map((track) => track.readyState),
        enabled: tracks.map((track) => track.enabled)
      };
    })
    .sort((left, right) => left.userId.localeCompare(right.userId));
}
