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
import { createPeerConnection } from "./webrtc";

type ServerMediaAudioSessionInput = {
  roomId: string;
  userId: string;
  accessToken: string;
  socket: WebSocket;
  audioTrackId?: string;
  iceServers: IceServerConfig[];
  stream: MediaStream;
  userAudio?: Record<string, UserAudioSettings>;
  pollIntervalMs?: number;
  onError?: (message: string) => void;
  onConnectionInterrupted?: () => void;
};

type RemotePlayback = {
  stream: MediaStream;
  source: MediaStreamAudioSourceNode;
  gain: GainNode;
};

const DEFAULT_AUDIO_TRACK_ID = "audio-main";
const DEFAULT_CANDIDATE_POLL_INTERVAL_MS = 1_000;
const SOCKET_NOT_CONNECTED_ERROR = "Audio signalling websocket is not connected";

export class ServerMediaAudioSession {
  private readonly audioTrackId: string;
  private readonly peer: RTCPeerConnection;
  private readonly seenCandidates = new Set<string>();
  private readonly pendingLocalCandidates: RTCIceCandidateInit[] = [];
  private readonly remotePlayback = new Map<string, RemotePlayback>();
  private candidatePoll?: number;
  private audioContext?: AudioContext;
  private offerAnswered = false;

  constructor(private readonly input: ServerMediaAudioSessionInput) {
    this.audioTrackId = input.audioTrackId ?? DEFAULT_AUDIO_TRACK_ID;
    this.peer = createPeerConnection(input.iceServers, input.stream);
    this.peer.onicecandidate = (event) => {
      if (event.candidate) {
        void this.sendOrQueueLocalCandidate(event.candidate.toJSON());
      }
    };
    this.peer.ontrack = (event) => {
      for (const track of event.streams[0]?.getTracks() ?? [event.track]) {
        this.addRemoteTrack(track);
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
    const answer = await answerServerMediaOffer(
      this.input.roomId,
      this.input.userId,
      this.audioTrackId,
      offer.sdp ?? "",
      this.input.accessToken
    );
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
    this.remotePlayback.delete(userId);
  }

  close(): void {
    if (this.candidatePoll !== undefined) {
      window.clearInterval(this.candidatePoll);
      this.candidatePoll = undefined;
    }
    this.peer.close();
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

  private addRemoteTrack(track: MediaStreamTrack): void {
    const sourceUserId = parseServerMediaSourceTrackId(track.id);
    if (!sourceUserId) {
      this.reportError(`Ignored server media track with invalid id: ${track.id}`);
      return;
    }
    this.removeUserAudio(sourceUserId);
    const stream = new MediaStream();
    stream.addTrack(track);
    const audioContext = this.audioContext ?? new AudioContext();
    this.audioContext = audioContext;
    const source = audioContext.createMediaStreamSource(stream);
    const gain = audioContext.createGain();
    source.connect(gain);
    gain.connect(audioContext.destination);
    if (audioContext.state === "suspended") {
      void audioContext.resume().catch((error: unknown) => this.reportError(error));
    }
    this.remotePlayback.set(sourceUserId, { stream, source, gain });
    this.setUserAudioSettings(
      sourceUserId,
      this.input.userAudio?.[sourceUserId] ?? { muted: false, volumePercent: 100 }
    );
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

function candidateKey(candidate: ServerMediaIceCandidate): string {
  return [
    candidate.candidate,
    candidate.sdp_mid ?? "",
    candidate.sdp_mline_index ?? "",
    candidate.username_fragment ?? ""
  ].join("|");
}
