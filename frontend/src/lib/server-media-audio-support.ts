import type { IceServerConfig, ServerMediaIceCandidate } from "./api";
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
  onPlaybackBlocked?: () => void;
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
