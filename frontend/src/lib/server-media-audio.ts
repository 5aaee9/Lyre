import {
  answerServerMediaOffer,
  type IceServerConfig,
  type ServerMediaIceCandidate
} from "./api";
import type { UserAudioSettings } from "./settings-store";
import {
  encodeServerMediaIceCandidate,
  encodeServerMediaIceCandidatesRequest,
  type SignalMessage
} from "./signalling";
import { VoiceActivityDetector } from "./voice-activity";
import { createPeerConnection } from "./webrtc";

type ServerMediaAudioSessionInput = {
  roomId: string;
  userId: string;
  accessToken: string;
  socket: WebSocket;
  audioTrackId?: string;
  iceServers: IceServerConfig[];
  stream: MediaStream;
  outputDeviceId?: string;
  userAudio?: Record<string, UserAudioSettings>;
  pollIntervalMs?: number;
  onError?: (message: string) => void;
  onConnectionInterrupted?: () => void;
  onRemoteTrack?: () => void;
  onRemoteSpeakingChange?: (userId: string, speaking: boolean) => void;
};

type RemotePlayback = {
  stream: MediaStream;
  source: MediaStreamAudioSourceNode;
  gain: GainNode;
  voiceActivity: VoiceActivityDetector;
};

export type ServerMediaAudioStats = {
  packetsSent: number;
  bytesSent: number;
  packetsReceived: number;
  bytesReceived: number;
  packetsLost: number;
  remotePacketsLost: number;
  roundTripTimeMs: number | null;
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
  lastPlaybackError: string | null;
  stats: ServerMediaAudioStats;
};

type DtxEncodingParameters = RTCRtpEncodingParameters & {
  dtx?: "disabled" | "enabled";
};

type DtxRtpSendParameters = RTCRtpSendParameters & {
  encodings: DtxEncodingParameters[];
};

type AudioContextWithSinkId = AudioContext & {
  setSinkId?: (sinkId: string) => Promise<void>;
};

const DEFAULT_AUDIO_TRACK_ID = "audio-main";
const DEFAULT_CANDIDATE_POLL_INTERVAL_MS = 1_000;
const SOCKET_NOT_CONNECTED_ERROR = "Audio signalling websocket is not connected";

const EMPTY_AUDIO_STATS: ServerMediaAudioStats = {
  packetsSent: 0,
  bytesSent: 0,
  packetsReceived: 0,
  bytesReceived: 0,
  packetsLost: 0,
  remotePacketsLost: 0,
  roundTripTimeMs: null
};

export class ServerMediaAudioSession {
  private readonly audioTrackId: string;
  private readonly peer: RTCPeerConnection;
  private readonly seenCandidates = new Set<string>();
  private readonly pendingLocalCandidates: RTCIceCandidateInit[] = [];
  private readonly remotePlayback = new Map<string, RemotePlayback>();
  private readonly sourceTrackIdsByMid = new Map<string, string>();
  private candidatePoll?: number;
  private audioContext?: AudioContext;
  private offerAnswered = false;
  private readonly onTrackTrackIds: string[] = [];
  private readonly rejectedTrackIds: string[] = [];
  private lastPlaybackError: string | null = null;

  constructor(private readonly input: ServerMediaAudioSessionInput) {
    this.audioTrackId = input.audioTrackId ?? DEFAULT_AUDIO_TRACK_ID;
    this.peer = createPeerConnection(input.iceServers, input.stream);
    this.peer.onicecandidate = (event) => {
      if (event.candidate) {
        void this.sendOrQueueLocalCandidate(event.candidate.toJSON());
      }
    };
    this.peer.ontrack = (event) => {
      const mid = event.transceiver?.mid ?? null;
      if (event.streams.length === 0) {
        this.addRemoteTrack(event.track, event.track.id, mid);
        return;
      }
      for (const stream of event.streams) {
        for (const track of stream.getTracks()) {
          this.addRemoteTrack(track, stream.id, mid);
        }
      }
    };
    this.peer.oniceconnectionstatechange = () => {
      if (this.peer.iceConnectionState === "disconnected" || this.peer.iceConnectionState === "failed") {
        this.input.onConnectionInterrupted?.();
      }
    };
  }

  async start(): Promise<void> {
    const offer = await this.peer.createOffer();
    await this.peer.setLocalDescription(offer);
    await this.enableOpusDtx();
    const answer = await answerServerMediaOffer(
      this.input.roomId,
      this.input.userId,
      this.audioTrackId,
      offer.sdp ?? "",
      this.input.accessToken
    );
    this.sourceTrackIdsByMid.clear();
    for (const [mid, trackId] of parseServerMediaSourceTrackIdsByMid(answer.sdp)) {
      this.sourceTrackIdsByMid.set(mid, trackId);
    }
    await this.peer.setRemoteDescription({ type: "answer", sdp: answer.sdp });
    this.offerAnswered = true;
    await this.flushLocalCandidates();
    this.requestServerCandidates({ report: false });
    this.candidatePoll = window.setInterval(() => {
      this.requestServerCandidates();
    }, this.input.pollIntervalMs ?? DEFAULT_CANDIDATE_POLL_INTERVAL_MS);
  }

  setMuted(muted: boolean): void {
    for (const track of this.input.stream.getAudioTracks()) {
      track.enabled = !muted;
    }
  }

  async resumePlayback(): Promise<void> {
    if (this.audioContext?.state === "suspended") {
      await this.audioContext.resume();
    }
  }

  async diagnostics(): Promise<ServerMediaAudioDiagnostics> {
    const stats = summarizeAudioStats(await this.peer.getStats());
    return {
      connectionState: this.peer.connectionState,
      iceConnectionState: this.peer.iceConnectionState,
      signalingState: this.peer.signalingState,
      audioContextState: this.audioContext?.state ?? "uncreated",
      remoteTrackIds: [...this.remotePlayback.values()].flatMap((playback) =>
        playback.stream.getAudioTracks().map((track) => track.id)
      ),
      receiverTrackIds: this.peer.getReceivers()
        .map((receiver) => receiver.track?.id)
        .filter((trackId): trackId is string => typeof trackId === "string" && trackId.length > 0),
      onTrackTrackIds: [...this.onTrackTrackIds],
      rejectedTrackIds: [...this.rejectedTrackIds],
      lastPlaybackError: this.lastPlaybackError,
      stats
    };
  }

  setUserAudioSettings(userId: string, settings: UserAudioSettings): void {
    const playback = this.remotePlayback.get(userId);
    if (!playback) {
      return;
    }
    const volumePercent = Math.min(150, Math.max(0, settings.volumePercent));
    playback.gain.gain.value = settings.muted ? 0 : volumePercent / 100;
  }

  removeUserAudio(userId: string): void {
    const playback = this.remotePlayback.get(userId);
    if (!playback) {
      return;
    }
    playback.source.disconnect();
    playback.gain.disconnect();
    playback.voiceActivity.stop();
    this.remotePlayback.delete(userId);
  }

  close(): void {
    if (this.candidatePoll !== undefined) {
      window.clearInterval(this.candidatePoll);
      this.candidatePoll = undefined;
    }
    this.peer.close();
    this.sourceTrackIdsByMid.clear();
    for (const track of this.input.stream.getAudioTracks()) {
      track.stop();
    }
    for (const userId of [...this.remotePlayback.keys()]) {
      this.removeUserAudio(userId);
    }
    void this.audioContext?.close();
    this.audioContext = undefined;
  }

  async handleSignal(signal: SignalMessage): Promise<void> {
    if (signal.payload.type !== "server-media-ice-candidates") {
      return;
    }
    try {
      for (const candidate of signal.payload.candidates) {
        await this.addServerCandidate(candidate);
      }
    } catch (error) {
      this.reportError(error);
    }
  }

  private async sendOrQueueLocalCandidate(candidate: RTCIceCandidateInit): Promise<void> {
    if (!this.offerAnswered) {
      this.pendingLocalCandidates.push(candidate);
      return;
    }
    await this.addLocalCandidate(candidate);
  }

  private async flushLocalCandidates(): Promise<void> {
    for (const candidate of this.pendingLocalCandidates.splice(0)) {
      await this.addLocalCandidate(candidate);
    }
  }

  private async addLocalCandidate(candidate: RTCIceCandidateInit): Promise<void> {
    try {
      this.sendSignal(encodeServerMediaIceCandidate(this.input.roomId, this.input.userId, candidate));
    } catch (error) {
      this.reportError(error);
    }
  }

  private addRemoteTrack(track: MediaStreamTrack, sourceId: string, mid: string | null): void {
    this.onTrackTrackIds.push(track.id);
    const sourceTrackId = (mid ? this.sourceTrackIdsByMid.get(mid) : undefined)
      ?? sourceId
      ?? track.id;
    const sourceUserId = parseServerMediaSourceTrackId(sourceTrackId) ?? parseServerMediaSourceTrackId(track.id);
    if (!sourceUserId) {
      this.rejectedTrackIds.push(track.id);
      this.reportPlaybackError(`Ignored server media track with invalid id: ${track.id}`);
      return;
    }
    this.removeUserAudio(sourceUserId);
    const stream = new MediaStream();
    stream.addTrack(track);
    const audioContext = this.audioContext ?? new AudioContext();
    this.audioContext = audioContext;
    this.applyOutputDevice(audioContext);
    const source = audioContext.createMediaStreamSource(stream);
    const voiceActivity = new VoiceActivityDetector(stream, (speaking) => {
      this.input.onRemoteSpeakingChange?.(sourceUserId, speaking);
    });
    voiceActivity.start();
    const gain = audioContext.createGain();
    source.connect(gain);
    gain.connect(audioContext.destination);
    if (audioContext.state === "suspended") {
      void audioContext.resume().catch((error: unknown) => this.reportPlaybackError(error));
    }
    this.remotePlayback.set(sourceUserId, { stream, source, gain, voiceActivity });
    this.setUserAudioSettings(
      sourceUserId,
      this.input.userAudio?.[sourceUserId] ?? { muted: false, volumePercent: 100 }
    );
    this.input.onRemoteTrack?.();
  }

  private applyOutputDevice(audioContext: AudioContext): void {
    const outputDeviceId = this.input.outputDeviceId;
    const setSinkId = (audioContext as AudioContextWithSinkId).setSinkId;
    if (!outputDeviceId || !setSinkId) {
      return;
    }
    void setSinkId.call(audioContext, outputDeviceId).catch((error: unknown) => {
      this.reportPlaybackError(error);
    });
  }

  private requestServerCandidates({ report = true }: { report?: boolean } = {}): void {
    try {
      this.sendSignal(encodeServerMediaIceCandidatesRequest(this.input.roomId, this.input.userId));
    } catch (error) {
      if (report) {
        this.reportError(error);
        return;
      }
      throw error;
    }
  }

  private async enableOpusDtx(): Promise<void> {
    const senders = this.peer.getSenders().filter((sender) => sender.track?.kind === "audio");
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

  private sendSignal(signal: SignalMessage): void {
    if (this.input.socket.readyState !== WebSocket.OPEN) {
      throw new Error(SOCKET_NOT_CONNECTED_ERROR);
    }
    this.input.socket.send(JSON.stringify(signal));
  }

  private async addServerCandidate(candidate: ServerMediaIceCandidate): Promise<void> {
    const key = candidateKey(candidate);
    if (this.seenCandidates.has(key)) {
      return;
    }
    this.seenCandidates.add(key);
    await this.peer.addIceCandidate({
      candidate: candidate.candidate,
      sdpMid: candidate.sdp_mid ?? undefined,
      sdpMLineIndex: candidate.sdp_mline_index ?? undefined,
      usernameFragment: candidate.username_fragment ?? undefined
    });
  }

  private reportError(error: unknown): void {
    if (error instanceof Error) {
      this.input.onError?.(error.message);
      return;
    }
    this.input.onError?.(typeof error === "string" ? error : "Audio connection failed");
  }

  private reportPlaybackError(error: unknown): void {
    const message = error instanceof Error
      ? error.message
      : typeof error === "string"
        ? error
        : "Audio playback failed";
    this.lastPlaybackError = message;
    this.input.onError?.(message);
  }
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

function candidateKey(candidate: ServerMediaIceCandidate): string {
  return [
    candidate.candidate,
    candidate.sdp_mid ?? "",
    candidate.sdp_mline_index ?? "",
    candidate.username_fragment ?? ""
  ].join("|");
}

function summarizeAudioStats(report: RTCStatsReport): ServerMediaAudioStats {
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
  roundTripTime?: number;
};

function isAudioRtpStats(stat: RTCStats): stat is AudioRtpStats {
  if (stat.type !== "inbound-rtp" && stat.type !== "outbound-rtp" && stat.type !== "remote-inbound-rtp") {
    return false;
  }
  return !("kind" in stat) || stat.kind === "audio";
}
