import {
  answerServerMediaOffer,
  type IceServerConfig,
  type ServerMediaIceCandidate
} from "./api";
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
  pollIntervalMs?: number;
  onError?: (message: string) => void;
  onConnectionInterrupted?: () => void;
};

const DEFAULT_AUDIO_TRACK_ID = "audio-main";
const DEFAULT_CANDIDATE_POLL_INTERVAL_MS = 1_000;
const SOCKET_NOT_CONNECTED_ERROR = "Audio signalling websocket is not connected";

export class ServerMediaAudioSession {
  private readonly audioTrackId: string;
  private readonly peer: RTCPeerConnection;
  private readonly remoteStream = new MediaStream();
  private readonly audio: HTMLAudioElement;
  private readonly seenCandidates = new Set<string>();
  private readonly pendingLocalCandidates: RTCIceCandidateInit[] = [];
  private candidatePoll?: number;
  private offerAnswered = false;

  constructor(private readonly input: ServerMediaAudioSessionInput) {
    this.audioTrackId = input.audioTrackId ?? DEFAULT_AUDIO_TRACK_ID;
    this.peer = createPeerConnection(input.iceServers, input.stream);
    this.audio = document.createElement("audio");
    this.audio.autoplay = true;
    this.audio.setAttribute("playsinline", "true");
    this.audio.srcObject = this.remoteStream;
    this.audio.hidden = true;
    document.body.append(this.audio);
    this.peer.onicecandidate = (event) => {
      if (event.candidate) {
        void this.sendOrQueueLocalCandidate(event.candidate.toJSON());
      }
    };
    this.peer.ontrack = (event) => {
      for (const track of event.streams[0]?.getTracks() ?? [event.track]) {
        this.remoteStream.addTrack(track);
      }
      void this.audio.play().catch((error: unknown) => this.reportError(error));
    };
    this.peer.oniceconnectionstatechange = () => {
      if (this.peer.iceConnectionState === "disconnected" || this.peer.iceConnectionState === "failed") {
        this.input.onConnectionInterrupted?.();
      }
    };
  }

  async start(): Promise<void> {
    void this.audio.play().catch((error: unknown) => this.reportError(error));
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

  close(): void {
    if (this.candidatePoll !== undefined) {
      window.clearInterval(this.candidatePoll);
      this.candidatePoll = undefined;
    }
    this.peer.close();
    for (const track of this.input.stream.getAudioTracks()) {
      track.stop();
    }
    this.audio.srcObject = null;
    this.audio.remove();
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
    this.input.onError?.(error instanceof Error ? error.message : "Audio connection failed");
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
