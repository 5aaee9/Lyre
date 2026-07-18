import {
  answerServerMediaOffer,
  type ServerMediaIceCandidate
} from "./api";
import type { UserAudioSettings } from "./settings-store";
import {
  applyRemotePlaybackSettings,
  candidateKey,
  createRemotePlayback,
  DEFAULT_AUDIO_TRACK_ID,
  DEFAULT_CANDIDATE_POLL_INTERVAL_MS,
  parseServerMediaSourceTrackId,
  parseServerMediaSourceTrackIdsByMid,
  SOCKET_NOT_CONNECTED_ERROR,
  summarizeAudioStats,
  type AudioContextWithSinkId,
  type DtxRtpSendParameters,
  type RemotePlayback,
  type ServerMediaAudioDiagnostics,
  type ServerMediaAudioSessionInput,
  type ServerMediaRemoteSourceDiagnostics
} from "./server-media-audio-support";
import {
  encodeServerMediaIceCandidate,
  encodeServerMediaIceCandidatesRequest,
  type SignalMessage
} from "./signalling";
import { createPeerConnection } from "./webrtc";

export {
  parseServerMediaSourceTrackId,
  parseServerMediaSourceTrackIdsByMid,
  type ServerMediaAudioDiagnostics,
  type ServerMediaAudioStats,
  type ServerMediaIceDiagnostics,
  type ServerMediaRemoteSourceDiagnostics
} from "./server-media-audio-support";

export class ServerMediaAudioSession {
  private readonly audioTrackId: string;
  private readonly peer: RTCPeerConnection;
  private readonly seenCandidates = new Set<string>();
  private readonly pendingLocalCandidates: RTCIceCandidateInit[] = [];
  private localCandidateCount = 0;
  private serverCandidateCount = 0;
  private lastServerCandidateCount = 0;
  private lastServerCandidateAt: string | null = null;
  private lastLocalCandidateAt: string | null = null;
  private lastServerCandidateError: string | null = null;
  private readonly remotePlayback = new Map<string, RemotePlayback>();
  private readonly sourceTrackIdsByMid = new Map<string, string>();
  private candidatePoll?: number;
  private audioContext?: AudioContext;
  private offerAnswered = false;
  private readonly onTrackTrackIds: string[] = [];
  private readonly rejectedTrackIds: string[] = [];
  private lastPlaybackError: string | null = null;
  private closed = false;

  constructor(private readonly input: ServerMediaAudioSessionInput) {
    this.audioTrackId = input.audioTrackId ?? DEFAULT_AUDIO_TRACK_ID;
    this.peer = createPeerConnection(input.iceServers, input.stream, {
      receiveOnlyAudio: input.listenOnly,
      remoteAudioSourceCount: Math.max(
        input.subscribedSourceIds?.length ?? 0,
        input.listenOnly ? 1 : 0
      )
    });
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
    const attempts = [
      ...(this.audioContext?.state === "suspended" ? [this.audioContext.resume()] : []),
      ...(this.audioContext ? [this.applyOutputDevice(this.audioContext)] : []),
      ...[...this.remotePlayback.values()].map((playback) => playback.element.play())
    ];
    try {
      await Promise.all(attempts);
      this.lastPlaybackError = null;
    } catch (error) {
      this.reportPlaybackError(error);
      throw error;
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
      ice: {
        localCandidateCount: this.localCandidateCount,
        serverCandidateCount: this.serverCandidateCount,
        lastServerCandidateCount: this.lastServerCandidateCount,
        lastServerCandidateAt: this.lastServerCandidateAt,
        lastLocalCandidateAt: this.lastLocalCandidateAt,
        lastServerCandidateError: this.lastServerCandidateError
      },
      remoteSources: this.remoteSourceDiagnostics(),
      lastPlaybackError: this.lastPlaybackError,
      stats
    };
  }

  setUserAudioSettings(userId: string, settings: UserAudioSettings): void {
    applyRemotePlaybackSettings(this.remotePlayback.get(userId), settings);
  }

  removeUserAudio(userId: string): void {
    const playback = this.remotePlayback.get(userId);
    if (!playback) {
      return;
    }
    this.remotePlayback.delete(userId);
    this.runCleanupStep(() => { playback.source.disconnect(); });
    this.runCleanupStep(() => { playback.gain.disconnect(); });
    this.runCleanupStep(() => { playback.voiceActivity.stop(); });
    this.runCleanupStep(() => { playback.element.remove(); });
  }

  close(): void {
    if (this.closed) {
      return;
    }
    this.closed = true;
    if (this.candidatePoll !== undefined) {
      window.clearInterval(this.candidatePoll);
      this.candidatePoll = undefined;
    }
    this.runCleanupStep(() => { this.peer.close(); });
    this.sourceTrackIdsByMid.clear();
    for (const track of this.input.stream.getAudioTracks()) {
      this.runCleanupStep(() => { track.stop(); });
    }
    for (const userId of [...this.remotePlayback.keys()]) {
      this.removeUserAudio(userId);
    }
    const audioContext = this.audioContext;
    this.audioContext = undefined;
    if (audioContext) {
      this.runCleanupStep(() => {
        void Promise.resolve(audioContext.close()).catch((error: unknown) => {
          this.reportCleanupError(error);
        });
      });
    }
  }

  async handleSignal(signal: SignalMessage): Promise<void> {
    if (signal.payload.type !== "server-media-ice-candidates") {
      return;
    }
    try {
      for (const candidate of signal.payload.candidates) {
        await this.addServerCandidate(candidate);
      }
      this.lastServerCandidateCount = signal.payload.candidates.length;
      this.lastServerCandidateAt = new Date().toISOString();
      this.lastServerCandidateError = null;
    } catch (error) {
      this.lastServerCandidateError = error instanceof Error ? error.message : "Failed to add server ICE candidate";
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
      this.localCandidateCount += 1;
      this.lastLocalCandidateAt = new Date().toISOString();
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
    const audioContext = this.audioContext ?? new AudioContext();
    this.audioContext = audioContext;
    void this.applyOutputDevice(audioContext).catch((error: unknown) => {
      this.reportPlaybackError(error);
    });
    this.remotePlayback.set(
      sourceUserId,
      createRemotePlayback(
        track,
        sourceUserId,
        audioContext,
        this.input.onRemoteSpeakingChange,
        (error) => this.reportPlaybackError(error)
      )
    );
    this.setUserAudioSettings(
      sourceUserId,
      this.input.userAudio?.[sourceUserId] ?? { muted: false, volumePercent: 100 }
    );
    this.input.onRemoteTrack?.();
  }

  private applyOutputDevice(audioContext: AudioContext): Promise<void> {
    const outputDeviceId = this.input.outputDeviceId;
    const setSinkId = (audioContext as AudioContextWithSinkId).setSinkId;
    if (!outputDeviceId || !setSinkId) {
      return Promise.resolve();
    }
    return setSinkId.call(audioContext, outputDeviceId);
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
    this.serverCandidateCount += 1;
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

  private runCleanupStep(step: () => void): void {
    try {
      step();
    } catch (error) {
      this.reportCleanupError(error);
    }
  }

  private reportCleanupError(error: unknown): void {
    try {
      this.reportError(error);
    } catch (reportError) {
      console.error("Server media audio cleanup error reporting failed", reportError);
    }
  }

  private reportPlaybackError(error: unknown): void {
    const message = error instanceof Error
      ? error.message
      : typeof error === "string"
        ? error
        : "Audio playback failed";
    this.lastPlaybackError = message;
    if (error instanceof Error && error.name === "NotAllowedError") {
      this.input.onPlaybackBlocked?.();
    }
    this.input.onError?.(message);
  }

  private remoteSourceDiagnostics(): ServerMediaRemoteSourceDiagnostics[] {
    return [...this.remotePlayback.entries()]
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
}
