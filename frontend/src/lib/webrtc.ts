import type { IceServerConfig } from "./api";
import { readAudioProcessingConfig } from "./storage";

export async function openLocalAudioStream(): Promise<MediaStream> {
  const audioProcessing = readAudioProcessingConfig();
  return navigator.mediaDevices.getUserMedia({
    audio: {
      echoCancellation: audioProcessing.echoCancellation,
      autoGainControl: audioProcessing.autoGainControl
    }
  });
}

export function createPeerConnection(iceServers: IceServerConfig[], stream: MediaStream): RTCPeerConnection {
  const connection = new RTCPeerConnection({
    iceServers: iceServers.map((server) => ({
      urls: server.urls,
      username: server.username ?? undefined,
      credential: server.credential ?? undefined
    }))
  });
  for (const track of stream.getAudioTracks()) {
    connection.addTrack(track, stream);
  }
  return connection;
}

export async function createAudioPeerConnection(iceServers: IceServerConfig[]): Promise<RTCPeerConnection> {
  return createPeerConnection(iceServers, await openLocalAudioStream());
}
