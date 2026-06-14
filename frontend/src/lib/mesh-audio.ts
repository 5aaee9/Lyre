import type { IceServerConfig, UserProfile } from "./api";
import { encodeAnswer, encodeIceCandidate, encodeOffer, type SignalMessage } from "./signalling";
import { createPeerConnection } from "./webrtc";

type MeshAudioSessionInput = {
  roomId: string;
  currentUserId: string;
  iceServers: IceServerConfig[];
  stream: MediaStream;
  send: (message: SignalMessage) => void;
  onError?: (message: string) => void;
};

export class MeshAudioSession {
  private readonly peers = new Map<string, RTCPeerConnection>();

  constructor(private readonly input: MeshAudioSessionInput) {}

  async connectToUsers(users: UserProfile[]): Promise<boolean> {
    let hasError = false;
    for (const user of users) {
      if (user.id !== this.input.currentUserId) {
        hasError = (await this.createOfferFor(user.id)) || hasError;
      }
    }
    return !hasError;
  }

  async handleSignal(signal: SignalMessage): Promise<void> {
    if (signal.sender_id === this.input.currentUserId) {
      return;
    }
    if (signal.recipient_id && signal.recipient_id !== this.input.currentUserId) {
      return;
    }

    try {
      if (signal.payload.type === "offer") {
        const peer = this.peerFor(signal.sender_id);
        await peer.setRemoteDescription({ type: "offer", sdp: signal.payload.sdp });
        const answer = await peer.createAnswer();
        await peer.setLocalDescription(answer);
        this.input.send(encodeAnswer(this.input.roomId, this.input.currentUserId, answer.sdp ?? "", signal.sender_id));
      }
      if (signal.payload.type === "answer") {
        const peer = this.peers.get(signal.sender_id);
        if (peer) {
          await peer.setRemoteDescription({ type: "answer", sdp: signal.payload.sdp });
        }
      }
      if (signal.payload.type === "ice-candidate") {
        const peer = this.peers.get(signal.sender_id);
        if (peer) {
          await peer.addIceCandidate({
            candidate: signal.payload.candidate,
            sdpMid: signal.payload.sdp_mid,
            sdpMLineIndex: signal.payload.sdp_m_line_index
          });
        }
      }
    } catch (error) {
      this.reportError(error);
    }
  }

  removePeer(userId: string): void {
    this.peers.get(userId)?.close();
    this.peers.delete(userId);
  }

  close(): void {
    for (const peer of this.peers.values()) {
      peer.close();
    }
    this.peers.clear();
    for (const track of this.input.stream.getAudioTracks()) {
      track.stop();
    }
  }

  private async createOfferFor(userId: string): Promise<boolean> {
    try {
      const peer = this.peerFor(userId);
      const offer = await peer.createOffer();
      await peer.setLocalDescription(offer);
      this.input.send(encodeOffer(this.input.roomId, this.input.currentUserId, offer.sdp ?? "", userId));
      return false;
    } catch (error) {
      this.reportError(error);
      return true;
    }
  }

  private peerFor(userId: string): RTCPeerConnection {
    const existing = this.peers.get(userId);
    if (existing) {
      return existing;
    }
    const peer = createPeerConnection(this.input.iceServers, this.input.stream);
    peer.onicecandidate = (event) => {
      if (event.candidate) {
        this.input.send(
          encodeIceCandidate(this.input.roomId, this.input.currentUserId, event.candidate.toJSON(), userId)
        );
      }
    };
    this.peers.set(userId, peer);
    return peer;
  }

  private reportError(error: unknown): void {
    this.input.onError?.(error instanceof Error ? error.message : "Audio connection failed");
  }
}
