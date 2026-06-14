import type { IceServerConfig } from "./api";

export async function createAudioPeerConnection(iceServers: IceServerConfig[]): Promise<RTCPeerConnection> {
  const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
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
